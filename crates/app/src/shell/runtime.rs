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
    hotkey: Option<super::hotkey::HotkeyBackend>,
    backends: std::collections::HashMap<&'static str, String>,
}

impl Global for Runtime {}

fn main_windows(cx: &App) -> Vec<AnyWindowHandle> {
    let mini = cx
        .try_global::<mini::MiniRegistry>()
        .and_then(|r| r.window());
    cx.windows()
        .into_iter()
        .filter(|w| Some(w.window_id()) != mini.map(|m| m.window_id()))
        .filter(|w| !crate::windows::is_tool_window(cx, w))
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
        hotkey: None,
        backends: std::collections::HashMap::new(),
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
    {
        let rt = cx.global::<Runtime>();
        let update = rt.p.settings.borrow().update.clone();
        match crate::update::start(&update, rt.p.inbox.clone()) {
            Ok(()) => tracing::info!("auto-update enabled"),
            Err(why) => tracing::info!(?why, "auto-update off"),
        }
    }
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
                let rt = cx.global_mut::<Runtime>();
                rt.hotkey = Some(backend);
                rt.backends.insert("global-hotkey", "registered".into());
            }
            Err(e) => {
                tracing::warn!("global shortcut unavailable: {e}");
                cx.global_mut::<Runtime>()
                    .backends
                    .insert("global-hotkey", format!("unavailable: {e}"));
            }
        }
    }
    let handle = tray::spawn(TrayModel::default(), inbox);
    cx.global_mut::<Runtime>().tray = Some(handle);
}

/// Handles one message: controller, then effects.
fn dispatch(cx: &mut App, msg: &Msg) {
    if let Msg::Backend { name, status } = msg {
        tracing::info!("shell backend {name}: {status}");
        cx.global_mut::<Runtime>()
            .backends
            .insert(name, status.clone());
    }
    if let Msg::UpdateReady { version } = msg {
        offer_update(cx, version);
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

/// Toast with "Reiniciar para actualizar" on the main window.
fn offer_update(cx: &mut App, version: &str) {
    let message = rust_i18n::t!("shell.update.ready", version = version).to_string();
    let label = rust_i18n::t!("shell.update.restart").to_string();
    if let Some(w) = main_windows(cx).into_iter().next() {
        let _ = w.update(cx, move |_, window, cx| {
            crate::ui::notify_action(
                window,
                cx,
                crate::ui::Level::Info,
                message,
                label,
                |window, cx| {
                    if let Err(e) = crate::update::apply_and_restart() {
                        crate::ui::notify(window, cx, crate::ui::Level::Error, e);
                    }
                },
            );
        });
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
        Effect::OpenSettings => crate::settings_view::open(cx),
        Effect::OpenFirstRun => crate::firstrun::open(cx),
        Effect::OpenAbout => crate::about::open(cx),
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

// ------------------------------------------------------------- settings screen helpers

/// Shared settings and their file (the settings screen edits them).
#[must_use]
pub fn settings_handle(cx: &App) -> Option<(Rc<RefCell<AppSettings>>, PathBuf)> {
    cx.try_global::<Runtime>()
        .map(|r| (r.p.settings.clone(), r.p.settings_path.clone()))
}

/// Last status a shortcut backend reported (`global-hotkey`, `portal`).
#[must_use]
pub fn backend_status(cx: &App, name: &str) -> Option<String> {
    cx.try_global::<Runtime>()
        .and_then(|r| r.backends.get(name).cloned())
}

/// Whether a tray icon is registered.
#[must_use]
pub fn tray_ready(cx: &App) -> bool {
    cx.try_global::<Runtime>().is_some_and(|r| r.state.tray)
}

/// "Keep running in the tray" changed.
pub fn set_keep_running(cx: &mut App, on: bool) {
    if !cx.has_global::<Runtime>() {
        return;
    }
    cx.global_mut::<Runtime>().state.keep_running = on;
    cx.set_quit_mode(if on {
        crate::ui::paint::QuitMode::Explicit
    } else {
        crate::ui::paint::QuitMode::LastWindowClosed
    });
}

/// Moves the global shortcut (X11 / Windows / macOS); on Wayland the portal owns the binding.
pub fn rebind_shortcut(cx: &mut App, shortcut: &str) -> Result<(), String> {
    match cx.try_global_mut_hotkey() {
        Some(h) => h.rebind(shortcut),
        None => Ok(()),
    }
}

/// Opens or closes the mini window now (the settings toggle).
pub fn set_mini_window(cx: &mut App, open: bool) {
    if cx.has_global::<Runtime>() && mini::is_open(cx) != open {
        toggle_mini(cx, open);
    }
}

/// The UI language changed: re-render everything and rebuild the menus.
pub fn language_changed(cx: &mut App) {
    cx.refresh_windows();
    if cx.has_global::<Runtime>() {
        install_menus(cx);
        push_tray(cx);
    }
}

trait HotkeyAccess {
    fn try_global_mut_hotkey(&mut self) -> Option<&mut super::hotkey::HotkeyBackend>;
}

impl HotkeyAccess for App {
    fn try_global_mut_hotkey(&mut self) -> Option<&mut super::hotkey::HotkeyBackend> {
        if self.has_global::<Runtime>() {
            self.global_mut::<Runtime>().hotkey.as_mut()
        } else {
            None
        }
    }
}

/// Sends a command through the shell inbox (buttons, menus).
pub fn send_command(cx: &App, command: super::ipc::Command) {
    if let Some(rt) = cx.try_global::<Runtime>() {
        let _ = rt.p.inbox.try_send(Msg::Command(command));
    }
}

/// Whether the shell runtime is installed (false in unit tests and the demo).
#[must_use]
pub fn is_running(cx: &App) -> bool {
    cx.try_global::<Runtime>().is_some()
}

/// Whether push-to-talk is held.
#[must_use]
pub fn is_listening(cx: &App) -> bool {
    cx.try_global::<Runtime>()
        .is_some_and(|r| r.state.ptt.listening())
}

/// Persists the layout and split ratio.
pub fn save_layout(cx: &App, layout: crate::views::layout::Layout, split_permille: u16) {
    if let Some((settings, path)) = settings_handle(cx) {
        let mut s = settings.borrow_mut();
        if s.ui.layout == layout && s.ui.split_permille == split_permille {
            return;
        }
        s.ui.layout = layout;
        s.ui.split_permille = split_permille;
        if let Err(e) = s.save(&path) {
            tracing::warn!("cannot save the layout: {e}");
        }
    }
}
