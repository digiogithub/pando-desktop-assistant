//! PDA-T-0025: always-on-top transparent mini window. `mini --kind normal|popup|floating|layer
//! [--opaque] [--secs N]` opens a 360x120 frameless window, counts frames / key presses / mouse
//! presses for N seconds and prints one JSON line.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
use gpui_kit::gpui::{
    FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, WindowBackgroundAppearance,
    WindowDecorations, WindowKind,
};
use gpui_kit::{
    App, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window, WindowBounds, WindowOptions, div, point, px, rgba, size,
};

#[derive(Default)]
struct Counters {
    frames: Cell<u32>,
    keys: Cell<u32>,
    clicks: Cell<u32>,
    drags_started: Cell<u32>,
    active_seen: Cell<bool>,
    decorations: Cell<&'static str>,
}

struct Mini {
    kind: String,
    counters: Rc<Counters>,
    focus: FocusHandle,
}

impl Render for Mini {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.counters.clone();
        c.frames.set(c.frames.get() + 1);
        if window.is_window_active() {
            c.active_seen.set(true);
        }
        c.decorations.set(match window.window_decorations() {
            gpui_kit::gpui::Decorations::Server => "server",
            gpui_kit::gpui::Decorations::Client { .. } => "client",
        });
        window.request_animation_frame();
        let (k, d) = (c.clone(), c.clone());
        div()
            .id("mini")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(move |_, _: &KeyDownEvent, _, _| k.keys.set(k.keys.get() + 1)))
            .size_full()
            .p(px(8.0))
            .child(
                div()
                    .size_full()
                    .rounded(px(16.0))
                    .bg(rgba(0x0F2A20E6))
                    .border_1()
                    .border_color(rgba(0xF4F1E81F))
                    .text_color(rgba(0xF4F1E8FF))
                    .p(px(14.0))
                    .child(format!("mini window: {} (frames {})", self.kind, c.frames.get()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _: &MouseDownEvent, window, _| {
                            d.clicks.set(d.clicks.get() + 1);
                            // Custom drag: needed because the window has no title bar.
                            d.drags_started.set(d.drags_started.get() + 1);
                            window.start_window_move();
                        }),
                    ),
            )
    }
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let get = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let kind = get("--kind").unwrap_or_else(|| "normal".into());
    let secs: u64 = get("--secs").and_then(|s| s.parse().ok()).unwrap_or(6);
    let opaque = args.iter().any(|a| a == "--opaque");
    let counters = Rc::new(Counters::default());
    let report_counters = counters.clone();
    let kind2 = kind.clone();
    let started = Instant::now();
    gpui_kit::application().run(move |cx: &mut App| {
        let bounds = gpui_kit::Bounds::new(point(px(200.0), px(200.0)), size(px(360.0), px(120.0)));
        let wk = match kind2.as_str() {
            "popup" => WindowKind::PopUp,
            "floating" => WindowKind::Floating,
            "layer" => WindowKind::LayerShell(LayerShellOptions {
                namespace: "pando-mini".into(),
                layer: Layer::Overlay,
                anchor: Anchor::TOP | Anchor::RIGHT,
                exclusive_zone: None,
                exclusive_edge: None,
                margin: Some((px(60.0), px(60.0), px(0.0), px(0.0))),
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
            }),
            _ => WindowKind::Normal,
        };
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            kind: wk,
            focus: true,
            show: true,
            is_movable: true,
            window_background: if opaque {
                WindowBackgroundAppearance::Opaque
            } else {
                WindowBackgroundAppearance::Transparent
            },
            window_decorations: Some(WindowDecorations::Client),
            app_id: Some("dev.pando.assistant.mini".into()),
            ..Default::default()
        };
        let counters = counters.clone();
        let kind3 = kind2.clone();
        match cx.open_window(opts, move |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            cx.new(|_| Mini { kind: kind3, counters, focus })
        }) {
            Ok(_) => {}
            Err(err) => {
                println!("{}", serde_json::json!({"spike":"mini","kind":kind2,"open_error":format!("{err:#}")}));
                cx.quit();
                return;
            }
        }
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(secs)).await;
            cx.update(|cx| cx.quit());
        })
        .detach();
    });
    let c = report_counters;
    println!(
        "{}",
        serde_json::json!({
            "spike": "mini", "kind": kind, "transparent": !opaque,
            "frames": c.frames.get(), "fps": c.frames.get() as f64 / started.elapsed().as_secs_f64(),
            "active_seen": c.active_seen.get(), "key_events": c.keys.get(), "mouse_downs": c.clicks.get(),
            "decorations": c.decorations.get(),
        })
    );
    Ok(())
}
