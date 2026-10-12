//! GPUI side of the shell: drains the inbox, runs the controller and performs its effects
//! (windows, mini window, tray, notifications, quit). Installed once from `app::start`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use async_channel::{Receiver, Sender};
use organism::{BodyState, CellInput};

use super::controller::{Effect, Msg, Output, ShellState};
use super::mini::{self, MiniContext, MiniData};
use super::notify::{self, CellWatcher, NotifyEvent};
use super::tray::{self, TrayHandle, TrayModel};
use crate::settings::AppSettings;
use crate::ui::{AnyWindowHandle, App, Global, Window};

/// Opens the main window and returns its handle.
pub type MainOpener = Rc<dyn Fn(&mut App) -> Option<AnyWindowHandle>>;

/// Everything the runtime needs from `app::start`.
#[allow(missing_debug_implementations)]
pub struct Params {
    /// Inbox sender (triggers clone it).
    pub inbox: Sender<Msg>,
    /// Settings, shared with the rest of the app.
    pub settings: Rc<RefCell<AppSettings>>,
    /// Where the settings are saved.
    pub settings_path: PathBuf,
    /// Opens the main window.
    pub open_main: MainOpener,
    /// Signals for the voice orchestrator.
    pub outputs: Sender<Output>,
    /// The user keymap file.
    pub keymap_path: PathBuf,
}

#[allow(missing_debug_implementations)]
struct Runtime {
    state: ShellState,
    p: Params,
    tray: Option<TrayHandle>,
    tray_model: TrayModel,
    watcher: CellWatcher,
    notify_when_done: HashSet<String>,
    was_connected: bool,
    reopen_mini: bool,
}

impl Global for Runtime {}

fn main_windows(cx: &App) -> Vec<AnyWindowHandle> {
    let mini = cx
        .try_global::<mini::MiniRegistry>()
        .and_then(|r| r.window());
    cx.windows()
        .into_iter()
        .filter(|w| Some(w.window_id()) != mini.map(|m| m.window_id()))
        .collect()
}

/// Installs the runtime: global shortcuts / tray backends, the inbox pump and the key bindings.
pub fn install(cx: &mut App, rx: Receiver<Msg>, p: Params) {
    let keep = p.settings.borrow().shell.keep_running;
    cx.set_global(Runtime {
        state: ShellState::new(keep),
        tray: None,
        tray_model: TrayModel::default(),
        watcher: CellWatcher::new(),
        notify_when_done: HashSet::new(),
        was_connected: false,
        reopen_mini: false,
        p,
    });
    {
        let rt = cx.global::<Runtime>();
        super::keymap::install(cx, rt.p.inbox.clone(), Some(&rt.p.keymap_path.clone()));
    }
    if keep {
        cx.set_quit_mode(crate::ui::paint::QuitMode::Explicit);
        cx.on_window_closed(|cx, _| {
            let background = cx
                .try_global::<Runtime>()
                .is_some_and(|r| r.state.background_mode());
            if main_windows(cx).is_empty() && !background {
                cx.quit();
            }
        })
        .detach();
    }
    start_backends(cx);
    install_menus(cx);
    if cx
        .global::<Runtime>()
        .p
        .settings
        .borrow()
        .shell
        .mini_enabled
    {
        toggle_mini(cx, true);
    }
    cx.spawn(async move |cx| {
        while let Ok(msg) = rx.recv().await {
            cx.update(|cx| dispatch(cx, &msg));
        }
    })
    .detach();
}

fn install_menus(cx: &mut App) {
    use super::keymap::{ShowMain, ToggleMini};
    use crate::ui::{MenuEntry, MenuSpec};
    crate::ui::set_menus(
        cx,
        vec![MenuSpec {
            name: "Pando".into(),
            items: vec![
                MenuEntry::Action(
                    crate::organism_view::tr("shell.tray.open"),
                    Box::new(ShowMain),
                ),
                MenuEntry::Action(
                    crate::organism_view::tr("shell.tray.mini"),
                    Box::new(ToggleMini),
                ),
            ],
        }],
    );
}

fn start_backends(cx: &mut App) {
    let (inbox, shortcut) = {
        let rt = cx.global::<Runtime>();
        (
            rt.p.inbox.clone(),
            rt.p.settings.borrow().shell.ptt_shortcut.clone(),
        )
    };
    // Hold-to-talk: portal on Wayland, global-hotkey elsewhere. Failures are reported, the CLI
    // trigger (`pando-assistant ptt ...`) always works.
    let wayland = super::portal::current_session() == super::portal::Session::Wayland;
    if wayland {
        #[cfg(target_os = "linux")]
        super::portal::spawn(&shortcut, inbox.clone());
    } else {
        match super::hotkey::HotkeyBackend::start(&shortcut, inbox.clone()) {
            Ok(backend) => {
                tracing::info!("global shortcut {shortcut:?} registered");
                // Lives for the whole process.
                std::mem::forget(backend);
            }
            Err(e) => tracing::warn!("global shortcut unavailable: {e}"),
        }
    }
    let handle = tray::spawn(TrayModel::default(), inbox);
    cx.global_mut::<Runtime>().tray = Some(handle);
}

/// Handles one message: controller, then effects.
fn dispatch(cx: &mut App, msg: &Msg) {
    if let Msg::Backend { name, status } = msg {
        tracing::info!("shell backend {name}: {status}");
    }
    let effects = cx.global_mut::<Runtime>().state.apply(msg);
    if matches!(msg, Msg::Tray(_)) {
        tracing::info!(
            "tray {}",
            if cx.global::<Runtime>().state.tray {
                "ready"
            } else {
                "unavailable"
            }
        );
    }
    for e in effects {
        perform(cx, e);
    }
}

fn perform(cx: &mut App, effect: Effect) {
    match effect {
        Effect::ShowMain => show_main(cx),
        Effect::HideMain => {
            let background = cx.global::<Runtime>().state.background_mode();
            for w in main_windows(cx) {
                let _ = w.update(cx, |_, window, _| {
                    if background {
                        window.remove_window();
                    } else {
                        window.minimize_window();
                    }
                });
            }
        }
        Effect::ToggleMini => {
            let open = mini::is_open(cx);
            toggle_mini(cx, !open);
        }
        Effect::FocusCell(id) => {
            show_main(cx);
            crate::cells::pane::select_cell(cx, &id);
        }
        Effect::Quit => cx.quit(),
        Effect::Output(o) => {
            let rt = cx.global_mut::<Runtime>();
            let _ = rt.p.outputs.try_send(o);
            match o {
                Output::Ptt(edge) => rt.tray_model.listening = edge == super::ptt::PttEdge::Started,
                Output::HandsFree(on) => rt.tray_model.hands_free = on,
                Output::Muted(on) => rt.tray_model.muted = on,
            }
            push_tray(cx);
        }
    }
}

fn push_tray(cx: &mut App) {
    let rt = cx.global::<Runtime>();
    if let Some(t) = &rt.tray {
        t.update(rt.tray_model);
    }
    tray::apply_pending();
}

fn show_main(cx: &mut App) {
    let windows = main_windows(cx);
    if windows.is_empty() {
        let open = cx.global::<Runtime>().p.open_main.clone();
        let _ = open(cx);
    } else {
        for w in windows {
            let _ = w.update(cx, |_, window, _| window.activate_window());
        }
    }
    cx.activate(true);
}

fn toggle_mini(cx: &mut App, open: bool) {
    set_mini(cx, open, true);
}

fn set_mini(cx: &mut App, open: bool, persist: bool) {
    if open {
        let (inbox, hotkey, remembered, settings, path) = {
            let rt = cx.global::<Runtime>();
            let s = rt.p.settings.borrow();
            (
                rt.p.inbox.clone(),
                s.shell.ptt_shortcut.clone(),
                s.shell.mini_position,
                rt.p.settings.clone(),
                rt.p.settings_path.clone(),
            )
        };
        let remember: Rc<dyn Fn(i32, i32)> = {
            let (settings, path) = (settings.clone(), path.clone());
            Rc::new(move |x, y| {
                let mut s = settings.borrow_mut();
                s.shell.mini_position = Some((x, y));
                if let Err(e) = s.save(&path) {
                    tracing::warn!("cannot save the mini window position: {e}");
                }
            })
        };
        mini::open(
            cx,
            MiniContext {
                inbox,
                hotkey,
                remembered,
                remember,
            },
        );
    } else {
        mini::close(cx);
    }
    if !persist {
        return;
    }
    let rt = cx.global::<Runtime>();
    let mut s = rt.p.settings.borrow_mut();
    s.shell.mini_enabled = open;
    if let Err(e) = s.save(&rt.p.settings_path) {
        tracing::warn!("cannot save settings: {e}");
    }
}

/// The main window gained or lost focus: with `mini_auto_hide` the mini window steps aside
/// while the main one is in use and comes back afterwards.
pub fn on_main_activation(cx: &mut App, active: bool) {
    if !cx.has_global::<Runtime>() {
        return;
    }
    if !cx
        .global::<Runtime>()
        .p
        .settings
        .borrow()
        .shell
        .mini_auto_hide
    {
        return;
    }
    if active && mini::is_open(cx) {
        cx.global_mut::<Runtime>().reopen_mini = true;
        set_mini(cx, false, false);
    } else if !active && std::mem::take(&mut cx.global_mut::<Runtime>().reopen_mini) {
        set_mini(cx, true, false);
    }
}

// ------------------------------------------------------------------- app-facing helpers

/// What the cells pane tells the shell after every poll.
pub fn publish(cx: &mut App, cells: &[CellInput], pending: u32, state: BodyState) {
    if !cx.has_global::<Runtime>() {
        return;
    }
    let events = {
        let rt = cx.global_mut::<Runtime>();
        rt.was_connected = true;
        rt.tray_model.body = state;
        rt.tray_model.pending = pending;
        rt.watcher.observe(cells)
    };
    push_tray(cx);
    mini::publish(
        cx,
        MiniData {
            state,
            cells: cells.to_vec(),
            pending,
            transcript: String::new(),
        },
    );
    for e in events {
        os_notify(cx, &e);
    }
}

/// The connection to Pando went away.
pub fn connection_lost(cx: &mut App) {
    if !cx.has_global::<Runtime>() {
        return;
    }
    let was = std::mem::replace(&mut cx.global_mut::<Runtime>().was_connected, false);
    cx.global_mut::<Runtime>().watcher.reset();
    if was {
        os_notify(cx, &NotifyEvent::ConnectionLost);
    }
}

/// Ask to be told when `cell` finishes (the card's "notify me" action).
pub fn notify_when_done(cx: &mut App, cell: &str) {
    if cx.has_global::<Runtime>() {
        cx.global_mut::<Runtime>()
            .notify_when_done
            .insert(cell.to_owned());
    }
}

fn os_notify(cx: &mut App, event: &NotifyEvent) {
    let rt = cx.global::<Runtime>();
    let enabled = rt.p.settings.borrow().shell.system_notifications;
    let focused = cx.active_window().is_some();
    if let Some(n) = notify::decide(event, enabled, focused, &rt.notify_when_done) {
        notify::show(n, rt.p.inbox.clone());
    }
}

/// Title-bar close: keep the app in the tray when that is possible, else quit.
pub fn close_window(window: &mut Window, cx: &mut App) {
    let background = cx
        .try_global::<Runtime>()
        .is_some_and(|r| r.state.background_mode());
    if background {
        window.remove_window();
    } else {
        cx.quit();
    }
}

/// Whether closing the window keeps the process alive.
#[must_use]
pub fn background_mode(cx: &App) -> bool {
    cx.try_global::<Runtime>()
        .is_some_and(|r| r.state.background_mode())
}
