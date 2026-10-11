//! PDA-T-0026: global hotkey (Pressed/Released) + tray + XDG GlobalShortcuts portal, all feeding
//! the GPUI foreground executor through one `async_channel` (the `EventPump` shape of the app).
//! `hotkey [--secs N] [--backends x11,portal,tray]`; prints one JSON line.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use async_channel::Sender;
use gpui_kit::App;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
enum Ev {
    /// A backend finished its setup: name and result text.
    Status(&'static str, String),
    Hotkey { backend: &'static str, pressed: bool, sent: Instant },
    TrayActivate(Instant),
    TrayMenu(&'static str),
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let secs: u64 = get("--secs").and_then(|s| s.parse().ok()).unwrap_or(8);
    let backends = get("--backends").unwrap_or_else(|| "x11,portal,tray".into());
    let (tx, rx) = async_channel::unbounded::<Ev>();
    let report: Rc<RefCell<Value>> = Rc::default();
    let out = report.clone();

    gpui_kit::application().run(move |cx: &mut App| {
        // ---- backends: each runs outside the GPUI thread and only sends into `tx` ----
        if backends.contains("x11") {
            x11_backend(tx.clone());
        }
        if backends.contains("portal") {
            portal_backend(tx.clone());
        }
        if backends.contains("tray") {
            tray_backend(tx.clone());
        }
        drop(tx);

        // ---- GPUI side: drain the channel on the foreground executor ----
        let state = Rc::new(RefCell::new(json!({
            "status": {}, "hotkey_events": [], "tray_activations": 0, "tray_menu": [],
            "max_pump_latency_us": 0u64, "ptt_state": "idle", "ptt_transitions": []
        })));
        let pump_state = state.clone();
        cx.spawn(async move |_cx| {
            while let Ok(ev) = rx.recv().await {
                let mut s = pump_state.borrow_mut();
                match ev {
                    Ev::Status(name, text) => s["status"][name] = json!(text),
                    Ev::Hotkey { backend, pressed, sent } => {
                        let lat = sent.elapsed().as_micros() as u64;
                        if lat > s["max_pump_latency_us"].as_u64().unwrap_or(0) {
                            s["max_pump_latency_us"] = json!(lat);
                        }
                        s["hotkey_events"].as_array_mut().map(|a| a.push(json!({"backend": backend, "pressed": pressed, "pump_us": lat})));
                        // Hold-to-talk state machine: Pressed -> listening, Released -> idle.
                        let next = if pressed { "listening" } else { "idle" };
                        s["ptt_state"] = json!(next);
                        s["ptt_transitions"].as_array_mut().map(|a| a.push(json!(next)));
                    }
                    Ev::TrayActivate(sent) => {
                        let n = s["tray_activations"].as_u64().unwrap_or(0) + 1;
                        s["tray_activations"] = json!(n);
                        let _ = sent;
                    }
                    Ev::TrayMenu(m) => s["tray_menu"].as_array_mut().map(|a| a.push(json!(m))).unwrap_or(()),
                }
            }
        })
        .detach();

        let end_state = state.clone();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(secs)).await;
            *out.borrow_mut() = end_state.borrow().clone();
            cx.update(|cx| cx.quit());
        })
        .detach();
    });
    println!("{}", json!({"spike": "hotkey", "result": &*report.borrow()}));
    Ok(())
}

// ---- X11 / macOS / Windows: global-hotkey crate ---------------------------------------------

fn x11_backend(tx: Sender<Ev>) {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    // The manager must live as long as the app (it owns the grab); leak it for the spike.
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.try_send(Ev::Status("global-hotkey", format!("manager error: {e}")));
            return;
        }
    };
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP);
    match manager.register(hotkey) {
        Ok(()) => {
            let _ = tx.try_send(Ev::Status("global-hotkey", "registered Ctrl+Alt+P".into()));
        }
        Err(e) => {
            let _ = tx.try_send(Ev::Status("global-hotkey", format!("register error: {e}")));
        }
    }
    let id = hotkey.id();
    let handler_tx = tx.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.id == id {
            let _ = handler_tx.try_send(Ev::Hotkey {
                backend: "global-hotkey",
                pressed: e.state == HotKeyState::Pressed,
                sent: Instant::now(),
            });
        }
    }));
    std::mem::forget(manager);
}

// ---- Wayland: XDG GlobalShortcuts portal via ashpd -------------------------------------------

fn portal_backend(tx: Sender<Ev>) {
    std::thread::spawn(move || {
        futures_lite::future::block_on(async move {
            use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
            use futures_util::StreamExt as _;
            let portal = match GlobalShortcuts::new().await {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(Ev::Status("portal", format!("unavailable: {e}"))).await;
                    return;
                }
            };
            let session = match portal.create_session(Default::default()).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Ev::Status("portal", format!("create_session: {e}"))).await;
                    return;
                }
            };
            let shortcuts =
                [NewShortcut::new("ptt", "Hold to talk to Pando").preferred_trigger("CTRL+ALT+p")];
            match portal.bind_shortcuts(&session, &shortcuts, None, Default::default()).await {
                Ok(_) => {
                    let _ = tx.send(Ev::Status("portal", "bound ptt (user may need to confirm)".into())).await;
                }
                Err(e) => {
                    let _ = tx.send(Ev::Status("portal", format!("bind_shortcuts: {e}"))).await;
                    return;
                }
            }
            let (Ok(act), Ok(deact)) = (portal.receive_activated().await, portal.receive_deactivated().await) else {
                return;
            };
            let mut events = futures_util::stream::select(act.map(|_| true), deact.map(|_| false));
            while let Some(pressed) = events.next().await {
                let _ = tx.send(Ev::Hotkey { backend: "portal", pressed, sent: Instant::now() }).await;
            }
        });
    });
}

// ---- Tray: ksni (StatusNotifierItem) --------------------------------------------------------

#[derive(Debug)]
struct SpikeTray {
    tx: Sender<Ev>,
}

impl ksni::Tray for SpikeTray {
    fn id(&self) -> String {
        "dev.pando.assistant.spike".into()
    }
    fn title(&self) -> String {
        "Pando (spike)".into()
    }
    fn icon_name(&self) -> String {
        "audio-input-microphone".into()
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.try_send(Ev::TrayActivate(Instant::now()));
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let item = |label: &str, tag: &'static str| {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |t: &mut Self| {
                    let _ = t.tx.try_send(Ev::TrayMenu(tag));
                }),
                ..Default::default()
            }
            .into()
        };
        vec![item("Show Pando", "show"), item("Mute microphone", "mute"), item("Quit", "quit")]
    }
}

fn tray_backend(tx: Sender<Ev>) {
    std::thread::spawn(move || {
        futures_lite::future::block_on(async move {
            use ksni::TrayMethods as _;
            match (SpikeTray { tx: tx.clone() }).spawn().await {
                Ok(handle) => {
                    let _ = tx.send(Ev::Status("tray", "ksni registered".into())).await;
                    let _keep = handle;
                    std::future::pending::<()>().await;
                }
                Err(e) => {
                    let _ = tx.send(Ev::Status("tray", format!("ksni error: {e}"))).await;
                }
            }
        });
    });
}
