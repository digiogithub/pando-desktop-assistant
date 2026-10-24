//! The floating mini window (PDA-T-0118): 380x440, no title bar, transparent corners with a
//! Bosque rounded card (radius 32), the organism at 118 px, state, last words, the hotkey hint
//! and a pending badge. Which kind of window it is depends on the platform (spike T-0025):
//!
//! | Platform | Window |
//! |---|---|
//! | macOS, Windows | `PopUp` (topmost), custom drag with `start_window_move` |
//! | X11 | `PopUp` (override-redirect, notification type), custom drag |
//! | Wayland: KDE, COSMIC, wlroots | `LayerShell` overlay, top-right, keyboard on demand |
//! | Wayland: GNOME | normal frameless transparent window ("always on top" is the user's WM) |
//!
//! `LayerShellNotSupportedError` (GNOME/Mutter) falls back to the normal window at open time.
//! Layer surfaces cannot be dragged (GPUI exposes no margin setter): they stay anchored.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use organism::{BodyState, CellInput};
use rust_i18n::t;

use super::controller::Msg;
use super::ipc::Command;
use super::portal::Session;
use crate::organism_view::OrganismView;
use crate::ui::paint::MouseButton;
#[cfg(target_os = "linux")]
use crate::ui::paint::mini_window::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
use crate::ui::paint::mini_window::{WindowBackgroundAppearance, WindowKind};
use crate::ui::theme::palette;
use crate::ui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, Global,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Subscription,
    Window, WindowBounds, WindowOptions, div, h_flex, point, px, rgb, rgba, size, v_flex,
};

/// Window size in logical px.
pub const SIZE: (f32, f32) = (380.0, 440.0);
/// Orbit diameter inside the window.
pub const ORGANISM_PX: f32 = 118.0;
/// Frame interval while the window is not focused (33-50 ms budget).
pub const INACTIVE_INTERVAL: Duration = Duration::from_millis(40);

/// Operating system family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    /// Linux and other Unix.
    Linux,
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
}

impl Os {
    /// The running OS.
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// How the mini window is created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniKind {
    /// Topmost pop-up (macOS, Windows, X11).
    Popup,
    /// Wayland layer-shell overlay.
    Layer,
    /// Plain frameless window (GNOME Wayland, and the fallback).
    Normal,
}

/// Picks the window kind for a platform (`desktop` is `XDG_CURRENT_DESKTOP`).
#[must_use]
pub fn choose_kind(os: Os, session: Session, desktop: &str) -> MiniKind {
    match (os, session) {
        (Os::MacOs | Os::Windows, _) | (Os::Linux, Session::Other) => MiniKind::Popup,
        (Os::Linux, Session::Wayland) => {
            if desktop.to_ascii_uppercase().contains("GNOME") {
                MiniKind::Normal
            } else {
                MiniKind::Layer
            }
        }
    }
}

/// Where the window goes: the remembered position if still on a screen, else top-right with a
/// margin. `screen` is the primary display size.
#[must_use]
pub fn initial_origin(remembered: Option<(i32, i32)>, screen: (f32, f32)) -> (f32, f32) {
    if let Some((x, y)) = remembered {
        let (x, y) = (x as f32, y as f32);
        if x > -SIZE.0 + 40.0 && y > -40.0 && x < screen.0 - 40.0 && y < screen.1 - 40.0 {
            return (x, y);
        }
    }
    ((screen.0 - SIZE.0 - 24.0).max(0.0), 48.0)
}

/// Data the pane publishes for the mini window.
#[derive(Clone, Debug, Default)]
pub struct MiniData {
    /// Body state.
    pub state: BodyState,
    /// Cells.
    pub cells: Vec<CellInput>,
    /// Pending approvals / questions.
    pub pending: u32,
    /// Last transcript line.
    pub transcript: String,
}

/// The open mini window, if any.
#[allow(missing_debug_implementations)]
struct MiniSlot {
    window: AnyWindowHandle,
    view: Entity<MiniView>,
}

/// Global registry of the mini window and what to show in it.
#[derive(Default)]
#[allow(missing_debug_implementations)]
pub struct MiniRegistry {
    slot: Option<MiniSlot>,
    data: MiniData,
}

impl Global for MiniRegistry {}

impl MiniRegistry {
    /// Handle of the open window.
    #[must_use]
    pub fn window(&self) -> Option<AnyWindowHandle> {
        self.slot.as_ref().map(|s| s.window)
    }

    /// Whether the window is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.slot.is_some()
    }
}

/// What the mini window needs from the app.
#[derive(Clone)]
#[allow(missing_debug_implementations)]
pub struct MiniContext {
    /// Shell inbox (click on the card shows the main window).
    pub inbox: async_channel::Sender<Msg>,
    /// Hotkey text shown in the hint.
    pub hotkey: String,
    /// Remembered position.
    pub remembered: Option<(i32, i32)>,
    /// Called with the new position (debounced) to persist it.
    pub remember: Rc<dyn Fn(i32, i32)>,
}

/// Publishes new data; updates the open window.
pub fn publish(cx: &mut App, data: MiniData) {
    if !cx.has_global::<MiniRegistry>() {
        cx.set_global(MiniRegistry::default());
    }
    let view = {
        let reg = cx.global_mut::<MiniRegistry>();
        reg.data = data.clone();
        reg.slot.as_ref().map(|s| s.view.clone())
    };
    if let Some(view) = view {
        view.update(cx, |v, cx| v.apply(&data, cx));
    }
}

/// Whether the mini window is open.
#[must_use]
pub fn is_open(cx: &App) -> bool {
    cx.try_global::<MiniRegistry>()
        .is_some_and(MiniRegistry::is_open)
}

/// Closes the window.
pub fn close(cx: &mut App) {
    let slot = cx
        .try_global::<MiniRegistry>()
        .is_some()
        .then(|| cx.global_mut::<MiniRegistry>().slot.take())
        .flatten();
    if let Some(slot) = slot {
        let _ = slot
            .window
            .update(cx, |_, window, _| window.remove_window());
    }
}

fn options(kind: MiniKind, origin: (f32, f32)) -> WindowOptions {
    let bounds = Bounds::new(
        point(px(origin.0), px(origin.1)),
        size(px(SIZE.0), px(SIZE.1)),
    );
    let kind = match kind {
        MiniKind::Popup => WindowKind::PopUp,
        MiniKind::Normal => WindowKind::Normal,
        // Layer shell only exists on Linux/Wayland; elsewhere it is a normal window.
        #[cfg(not(target_os = "linux"))]
        MiniKind::Layer => WindowKind::Normal,
        #[cfg(target_os = "linux")]
        MiniKind::Layer => WindowKind::LayerShell(LayerShellOptions {
            namespace: "pando-mini".into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::RIGHT,
            exclusive_zone: None,
            exclusive_edge: None,
            margin: Some((px(origin.1), px(24.0), px(0.0), px(origin.0))),
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
    };
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        kind,
        focus: false,
        show: true,
        is_movable: true,
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(crate::ui::frameless::WindowDecorations::Client),
        app_id: Some(format!("{}.mini", crate::views::title_bar::APP_ID)),
        ..Default::default()
    }
}

/// Opens the mini window (no-op when open). Layer shell failures fall back to a normal window.
pub fn open(cx: &mut App, mc: MiniContext) {
    if is_open(cx) {
        return;
    }
    if !cx.has_global::<MiniRegistry>() {
        cx.set_global(MiniRegistry::default());
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let mut kind = choose_kind(Os::current(), super::portal::current_session(), &desktop);
    let screen = cx.primary_display().map_or((1920.0, 1080.0), |d| {
        let s = d.bounds().size;
        (f32::from(s.width), f32::from(s.height))
    });
    let origin = initial_origin(mc.remembered, screen);
    let data = cx.global::<MiniRegistry>().data.clone();
    loop {
        let mc2 = mc.clone();
        let data2 = data.clone();
        let opened = cx.open_window(options(kind, origin), move |window, cx| {
            cx.new(|cx| MiniView::new(window, cx, mc2, &data2, kind))
        });
        match opened {
            Ok(handle) => {
                let view = handle
                    .downcast::<MiniView>()
                    .and_then(|h| h.entity(cx).ok());
                if let Some(view) = view {
                    cx.global_mut::<MiniRegistry>().slot = Some(MiniSlot {
                        window: handle.into(),
                        view,
                    });
                }
                return;
            }
            Err(err) if kind == MiniKind::Layer => {
                tracing::info!("layer shell unavailable ({err:#}); using a normal window");
                kind = MiniKind::Normal;
            }
            Err(err) => {
                tracing::warn!("cannot open the mini window: {err:#}");
                return;
            }
        }
    }
}

/// The mini window's root view.
#[allow(missing_debug_implementations)]
pub struct MiniView {
    organism: Entity<OrganismView>,
    state: BodyState,
    pending: u32,
    transcript: String,
    hotkey: String,
    inbox: async_channel::Sender<Msg>,
    movable: bool,
    _bounds_sub: Subscription,
    debounce: Rc<RefCell<u64>>,
}

impl MiniView {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        mc: MiniContext,
        data: &MiniData,
        kind: MiniKind,
    ) -> Self {
        let organism = cx.new(|cx| OrganismView::new(window, cx));
        organism.update(cx, |o, cx| {
            o.set_inactive_interval(INACTIVE_INTERVAL);
            o.set_cells(data.cells.clone(), cx);
            o.set_state(data.state, cx);
        });
        let debounce = Rc::new(RefCell::new(0_u64));
        let remember = mc.remember.clone();
        let gen_ = debounce.clone();
        let sub = cx.observe_window_bounds(window, move |_, window, cx| {
            let b = window.bounds();
            let (x, y) = (f32::from(b.origin.x) as i32, f32::from(b.origin.y) as i32);
            *gen_.borrow_mut() += 1;
            let mine = *gen_.borrow();
            let (gen2, remember) = (gen_.clone(), remember.clone());
            cx.spawn(async move |_this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(800))
                    .await;
                if *gen2.borrow() == mine {
                    remember(x, y);
                }
            })
            .detach();
        });
        Self {
            organism,
            state: data.state,
            pending: data.pending,
            transcript: data.transcript.clone(),
            hotkey: mc.hotkey,
            inbox: mc.inbox,
            movable: kind != MiniKind::Layer,
            _bounds_sub: sub,
            debounce,
        }
    }

    fn apply(&mut self, data: &MiniData, cx: &mut Context<Self>) {
        self.state = data.state;
        self.pending = data.pending;
        self.transcript.clone_from(&data.transcript);
        self.organism.update(cx, |o, cx| {
            o.set_cells(data.cells.clone(), cx);
            o.set_state(data.state, cx);
        });
        cx.notify();
    }
}

impl Render for MiniView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.debounce;
        let inbox = self.inbox.clone();
        let movable = self.movable;
        let state_label = crate::organism_view::demo::state_name(self.state);
        let badge = (self.pending > 0).then(|| {
            div()
                .px(px(8.0))
                .rounded_full()
                .bg(rgb(palette::BERMELLON))
                .text_size(px(11.0))
                .child(format!("? {}", self.pending))
        });
        let transcript = if self.transcript.is_empty() {
            t!("shell.mini.idle_hint").to_string()
        } else {
            self.transcript.clone()
        };
        div().id("mini-root").size_full().p(px(6.0)).child(
            v_flex()
                .id("mini-card")
                .size_full()
                .rounded(px(32.0))
                .bg(rgba(0x0F2A20F2))
                .border_1()
                .border_color(rgba(0xF4F1E81F))
                .text_color(rgb(palette::MARFIL))
                .p(px(16.0))
                .gap(px(8.0))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |_, _, window, _| {
                        if movable {
                            window.start_window_move();
                        }
                    }),
                )
                .on_mouse_up(MouseButton::Left, move |_, _, _| {
                    let _ = inbox.try_send(Msg::Command(Command::Show));
                })
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().text_size(px(13.0)).child(state_label))
                        .children(badge),
                )
                .child(
                    div()
                        .h(px(ORGANISM_PX + 40.0))
                        .w_full()
                        .child(self.organism.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.0))
                        .text_color(rgba(0xF4F1E8A0))
                        .child(transcript),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_family(crate::fonts::FONT_MONO)
                        .text_color(rgba(0xF4F1E873))
                        .child(t!("shell.mini.hotkey_hint", key = self.hotkey).to_string()),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_per_platform() {
        assert_eq!(choose_kind(Os::MacOs, Session::Other, ""), MiniKind::Popup);
        assert_eq!(
            choose_kind(Os::Windows, Session::Other, ""),
            MiniKind::Popup
        );
        assert_eq!(
            choose_kind(Os::Linux, Session::Other, "KDE"),
            MiniKind::Popup
        );
        assert_eq!(
            choose_kind(Os::Linux, Session::Wayland, "KDE"),
            MiniKind::Layer
        );
        assert_eq!(
            choose_kind(Os::Linux, Session::Wayland, "COSMIC"),
            MiniKind::Layer
        );
        assert_eq!(
            choose_kind(Os::Linux, Session::Wayland, "ubuntu:GNOME"),
            MiniKind::Normal
        );
    }

    #[test]
    fn origin_keeps_a_visible_remembered_position() {
        let screen = (1920.0, 1080.0);
        assert_eq!(initial_origin(Some((100, 80)), screen), (100.0, 80.0));
        // Off screen (monitor unplugged): back to the default top-right corner.
        assert_eq!(initial_origin(Some((5000, 80)), screen), (1516.0, 48.0));
        assert_eq!(initial_origin(None, screen), (1516.0, 48.0));
    }
}
