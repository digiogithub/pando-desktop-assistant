//! System tray (PDA-T-0123): `ksni` (StatusNotifierItem) on Linux, `tray-icon` on macOS and
//! Windows. The model, the menu and the icon pixels are pure and tested here; the backends only
//! forward menu clicks into the shell inbox as [`Command`]s.

use async_channel::Sender;
use organism::BodyState;
use rust_i18n::t;

use super::controller::Msg;
use super::ipc::{Command, PttAction};

/// What the tray shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrayModel {
    /// Body state of the organism (colours the icon).
    pub body: BodyState,
    /// Pending items (approvals, questions) waiting for the user.
    pub pending: u32,
    /// Push-to-talk is held.
    pub listening: bool,
    /// Hands-free listening.
    pub hands_free: bool,
    /// Voice muted.
    pub muted: bool,
}

/// One entry of the tray menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A clickable item; `checked` makes it a check item.
    Item {
        /// Localized label.
        label: String,
        /// Command sent on click.
        command: Command,
        /// Check state for toggles.
        checked: Option<bool>,
    },
    /// A separator line.
    Separator,
}

/// The menu: Escuchar, Manos libres, Silenciar voz, Abrir ventana, Mini ventana, Salir.
#[must_use]
pub fn menu(model: &TrayModel) -> Vec<Entry> {
    let item = |key: &str, command: Command, checked: Option<bool>| Entry::Item {
        label: crate::organism_view::tr(key),
        command,
        checked,
    };
    vec![
        item(
            "shell.tray.listen",
            Command::Ptt {
                action: PttAction::Toggle,
            },
            Some(model.listening),
        ),
        item(
            "shell.tray.hands_free",
            Command::ToggleHandsFree,
            Some(model.hands_free),
        ),
        item("shell.tray.mute", Command::ToggleMute, Some(model.muted)),
        Entry::Separator,
        item("shell.tray.open", Command::Show, None),
        item("shell.tray.mini", Command::ToggleMini, None),
        item("shell.tray.settings", Command::OpenSettings, None),
        Entry::Separator,
        item("shell.tray.quit", Command::Quit, None),
    ]
}

/// Tooltip text: app name, state, pending count.
#[must_use]
pub fn tooltip(model: &TrayModel) -> String {
    let state = crate::organism_view::demo::state_name(model.body);
    if model.pending > 0 {
        format!(
            "Pando · {state} · {}",
            t!("shell.tray.pending", count = model.pending)
        )
    } else {
        format!("Pando · {state}")
    }
}

/// Icon colour per body state (design tokens: Marfil idle, Álamo active, Claude blue thinking).
fn body_rgb(b: BodyState) -> [u8; 3] {
    use organism::tokens::{ALAMO, CLAUDE_BLUE, MARFIL};
    let c = match b {
        BodyState::Idle => MARFIL,
        BodyState::Thinking => CLAUDE_BLUE,
        BodyState::Listening | BodyState::Speaking | BodyState::Delegating => ALAMO,
    };
    [(c.rgb >> 16) as u8, (c.rgb >> 8) as u8, c.rgb as u8]
}

/// RGBA pixels (straight alpha) of a `size` x `size` icon: a ring (filled while listening) with a
/// red badge when items are pending. Anti-aliased by 4x4 supersampling.
#[must_use]
pub fn icon_rgba(model: &TrayModel, size: u32) -> Vec<u8> {
    let rgb = body_rgb(model.body);
    let badge = organism::tokens::BERMELLON;
    let badge_rgb = [
        (badge.rgb >> 16) as u8,
        (badge.rgb >> 8) as u8,
        badge.rgb as u8,
    ];
    let s = size as f32;
    let (cx, cy, r_out) = (s / 2.0, s / 2.0, s * 0.40);
    let r_in = if model.listening { 0.0 } else { s * 0.27 };
    let (bx, by, br) = (s * 0.76, s * 0.24, s * 0.20);
    let mut out = vec![0_u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (mut a_main, mut a_badge) = (0.0_f32, 0.0_f32);
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f32 + (sx as f32 + 0.5) / 4.0;
                    let py = y as f32 + (sy as f32 + 0.5) / 4.0;
                    let d = (px - cx).hypot(py - cy);
                    if d <= r_out && d >= r_in {
                        a_main += 1.0 / 16.0;
                    }
                    if model.pending > 0 && (px - bx).hypot(py - by) <= br {
                        a_badge += 1.0 / 16.0;
                    }
                }
            }
            let i = ((y * size + x) * 4) as usize;
            // The badge is painted over the ring.
            let total = a_badge + a_main * (1.0 - a_badge);
            if total > 0.0 {
                let mix = |m: u8, b: u8| {
                    ((f32::from(b) * a_badge + f32::from(m) * a_main * (1.0 - a_badge)) / total)
                        as u8
                };
                out[i] = mix(rgb[0], badge_rgb[0]);
                out[i + 1] = mix(rgb[1], badge_rgb[1]);
                out[i + 2] = mix(rgb[2], badge_rgb[2]);
                out[i + 3] = (total * 255.0).round() as u8;
            }
        }
    }
    out
}

/// Handle to push model updates to the running tray.
#[derive(Clone, Debug)]
pub struct TrayHandle {
    tx: Sender<TrayModel>,
}

impl TrayHandle {
    /// Tells the tray to redraw. Cheap; coalesced by the backend.
    pub fn update(&self, model: TrayModel) {
        let _ = self.tx.try_send(model);
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    struct PandoTray {
        model: TrayModel,
        inbox: Sender<Msg>,
    }

    impl PandoTray {
        fn send(&self, c: Command) {
            let _ = self.inbox.try_send(Msg::Command(c));
        }
    }

    impl ksni::Tray for PandoTray {
        fn id(&self) -> String {
            crate::views::title_bar::APP_ID.into()
        }
        fn title(&self) -> String {
            "Pando".into()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: tooltip(&self.model),
                ..Default::default()
            }
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            // SNI wants ARGB32 in network byte order.
            let size = 32;
            let rgba = icon_rgba(&self.model, size);
            let data = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[3], p[0], p[1], p[2]])
                .collect();
            vec![ksni::Icon {
                width: size as i32,
                height: size as i32,
                data,
            }]
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(Command::Show);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::{CheckmarkItem, StandardItem};
            menu(&self.model)
                .into_iter()
                .map(|e| match e {
                    Entry::Separator => ksni::MenuItem::Separator,
                    Entry::Item {
                        label,
                        command,
                        checked: Some(checked),
                    } => CheckmarkItem {
                        label,
                        checked,
                        activate: Box::new(move |t: &mut Self| t.send(command.clone())),
                        ..Default::default()
                    }
                    .into(),
                    Entry::Item { label, command, .. } => StandardItem {
                        label,
                        activate: Box::new(move |t: &mut Self| t.send(command.clone())),
                        ..Default::default()
                    }
                    .into(),
                })
                .collect()
        }
    }

    pub fn spawn(model: TrayModel, inbox: Sender<Msg>) -> TrayHandle {
        let (tx, rx) = async_channel::unbounded::<TrayModel>();
        let _ = std::thread::Builder::new()
            .name("pando-tray".into())
            .spawn(move || {
                futures_lite::future::block_on(async move {
                    use ksni::TrayMethods as _;
                    // The watcher can appear late (COSMIC, GNOME with the AppIndicator extension):
                    // retry a few times, and keep working without a tray if it never does.
                    let mut handle = None;
                    for attempt in 0..6_u64 {
                        match (PandoTray {
                            model,
                            inbox: inbox.clone(),
                        })
                        .spawn()
                        .await
                        {
                            Ok(h) => {
                                handle = Some(h);
                                break;
                            }
                            Err(e) => {
                                tracing::debug!(
                                    "tray registration failed (attempt {attempt}): {e}"
                                );
                                async_io_sleep(std::time::Duration::from_secs(2 + attempt * 2))
                                    .await;
                            }
                        }
                    }
                    let Some(handle) = handle else {
                        let _ = inbox.send(Msg::Tray(false)).await;
                        return;
                    };
                    let _ = inbox.send(Msg::Tray(true)).await;
                    while let Ok(mut m) = rx.recv().await {
                        // Coalesce bursts.
                        while let Ok(next) = rx.try_recv() {
                            m = next;
                        }
                        if handle.update(|t| t.model = m).await.is_none() {
                            break;
                        }
                    }
                    let _ = inbox.send(Msg::Tray(false)).await;
                });
            });
        TrayHandle { tx }
    }

    async fn async_io_sleep(d: std::time::Duration) {
        let (tx, rx) = async_channel::bounded::<()>(1);
        std::thread::spawn(move || {
            std::thread::sleep(d);
            let _ = tx.try_send(());
        });
        let _ = rx.recv().await;
    }
}

#[cfg(target_os = "linux")]
pub use linux::spawn;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod desktop {
    use std::cell::RefCell;

    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use super::*;

    /// Tray owned by the GPUI main thread (required on macOS). The menu is built once; model
    /// updates only change the icon, the tooltip and the check marks, so the click map stays
    /// valid.
    struct DesktopTray {
        icon: TrayIcon,
        checks: Vec<(Command, CheckMenuItem)>,
        updates: async_channel::Receiver<TrayModel>,
    }

    thread_local! {
        static TRAY: RefCell<Option<DesktopTray>> = const { RefCell::new(None) };
    }

    fn icon(model: &TrayModel) -> Option<Icon> {
        Icon::from_rgba(icon_rgba(model, 32), 32, 32).ok()
    }

    /// Creates the tray on the calling (main) thread; menu and icon clicks go to `inbox`.
    pub fn spawn(model: TrayModel, inbox: Sender<Msg>) -> TrayHandle {
        let (tx, updates) = async_channel::unbounded::<TrayModel>();
        let menu = Menu::new();
        let mut ids: Vec<(MenuId, Command)> = Vec::new();
        let mut checks = Vec::new();
        for e in super::menu(&model) {
            match e {
                Entry::Separator => {
                    let _ = menu.append(&PredefinedMenuItem::separator());
                }
                Entry::Item {
                    label,
                    command,
                    checked: Some(c),
                } => {
                    let it = CheckMenuItem::new(label, true, c, None);
                    ids.push((it.id().clone(), command.clone()));
                    let _ = menu.append(&it);
                    checks.push((command, it));
                }
                Entry::Item { label, command, .. } => {
                    let it = MenuItem::new(label, true, None);
                    ids.push((it.id().clone(), command));
                    let _ = menu.append(&it);
                }
            }
        }
        let Some(first_icon) = icon(&model) else {
            let _ = inbox.try_send(Msg::Tray(false));
            return TrayHandle { tx };
        };
        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(tooltip(&model))
            .with_icon(first_icon)
            .build()
        {
            Ok(icon) => {
                let menu_inbox = inbox.clone();
                MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                    if let Some((_, c)) = ids.iter().find(|(id, _)| *id == e.id) {
                        let _ = menu_inbox.try_send(Msg::Command(c.clone()));
                    }
                }));
                let click_inbox = inbox.clone();
                TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                    if let TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
                        button_state: tray_icon::MouseButtonState::Up,
                        ..
                    } = e
                    {
                        let _ = click_inbox.try_send(Msg::Command(Command::Show));
                    }
                }));
                TRAY.with(|t| {
                    *t.borrow_mut() = Some(DesktopTray {
                        icon,
                        checks,
                        updates,
                    });
                });
                let _ = inbox.try_send(Msg::Tray(true));
            }
            Err(e) => {
                tracing::warn!("tray unavailable: {e}");
                let _ = inbox.try_send(Msg::Tray(false));
            }
        }
        TrayHandle { tx }
    }

    /// Applies the latest queued model (call from the GPUI thread after `TrayHandle::update`,
    /// e.g. from the shell loop).
    pub fn apply_pending() {
        TRAY.with(|t| {
            let t = t.borrow();
            let Some(tray) = t.as_ref() else { return };
            let mut latest = None;
            while let Ok(m) = tray.updates.try_recv() {
                latest = Some(m);
            }
            let Some(model) = latest else { return };
            let _ = tray.icon.set_icon(icon(&model));
            let _ = tray.icon.set_tooltip(Some(tooltip(&model)));
            for (cmd, item) in &tray.checks {
                item.set_checked(match cmd {
                    Command::Ptt { .. } => model.listening,
                    Command::ToggleHandsFree => model.hands_free,
                    Command::ToggleMute => model.muted,
                    _ => item.is_checked(),
                });
            }
        });
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use desktop::{apply_pending, spawn};

/// Linux updates reach the tray through its own thread; nothing to apply on the GPUI side.
#[cfg(target_os = "linux")]
pub fn apply_pending() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() {
        rust_i18n::set_locale("en");
    }

    #[test]
    fn menu_has_the_required_entries_in_order() {
        en();
        let m = menu(&TrayModel::default());
        let labels: Vec<String> = m
            .iter()
            .filter_map(|e| match e {
                Entry::Item { label, .. } => Some(label.clone()),
                Entry::Separator => None,
            })
            .collect();
        assert_eq!(labels.len(), 7);
        assert!(
            labels.iter().all(|l| !l.starts_with("shell.tray")),
            "{labels:?}"
        );
        assert!(matches!(
            &m[0],
            Entry::Item {
                command: Command::Ptt {
                    action: PttAction::Toggle
                },
                checked: Some(false),
                ..
            }
        ));
        assert!(matches!(
            &m[8],
            Entry::Item {
                command: Command::Quit,
                ..
            }
        ));
    }

    #[test]
    fn toggles_reflect_the_model() {
        en();
        let m = menu(&TrayModel {
            hands_free: true,
            muted: true,
            ..TrayModel::default()
        });
        assert!(matches!(
            &m[1],
            Entry::Item {
                checked: Some(true),
                ..
            }
        ));
        assert!(matches!(
            &m[2],
            Entry::Item {
                checked: Some(true),
                ..
            }
        ));
    }

    #[test]
    fn icon_pixels_follow_state_and_badge() {
        let idle = icon_rgba(&TrayModel::default(), 32);
        assert_eq!(idle.len(), 32 * 32 * 4);
        let centre = (16 * 32 + 16) * 4;
        assert_eq!(idle[centre + 3], 0, "idle is a ring: hollow centre");
        let listening = icon_rgba(
            &TrayModel {
                listening: true,
                body: BodyState::Listening,
                ..TrayModel::default()
            },
            32,
        );
        assert_eq!(listening[centre + 3], 255, "listening is filled");
        let badge_px = (7 * 32 + 24) * 4;
        assert!(idle[badge_px + 1] > 200, "no badge when nothing is pending");
        let pending = icon_rgba(
            &TrayModel {
                pending: 2,
                ..TrayModel::default()
            },
            32,
        );
        assert!(pending[badge_px + 3] > 200);
        assert!(
            pending[badge_px] > 200 && pending[badge_px + 1] < 120,
            "badge is red"
        );
    }

    #[test]
    fn tooltip_mentions_pending_items() {
        en();
        assert!(
            tooltip(&TrayModel {
                pending: 3,
                ..TrayModel::default()
            })
            .contains('3')
        );
        assert!(!tooltip(&TrayModel::default()).contains("pending"));
    }
}
