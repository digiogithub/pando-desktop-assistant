//! The settings window (PDA-T-0126): sections Conexión, Voz, Audio, Atajos, Apariencia, Sistema
//! and Avanzado. Changes go through [`commit`] (validate, apply live, persist).
//!
//! Other work packages add or replace sections with [`register_section`]: the voice agent
//! registers `voice` (STT engine, language, TTS voice, speed, sensitivity) and `audio` (host,
//! devices, test) and the built-in placeholders step aside.

pub mod model;
mod view;

use std::rc::Rc;

use rust_i18n::t;

use crate::settings::AppSettings;
use crate::ui::{AnyElement, App, Global, Window};
use model::{Change, Invalid};

pub use view::SettingsView;

/// A settings section contributed by another module.
pub trait Section {
    /// Section id: one of [`model::SECTIONS`] to replace a built-in section, or a new id to add
    /// one at the end.
    fn id(&self) -> &'static str;
    /// Localized title (used for new sections; built-ins keep theirs).
    fn title(&self) -> String;
    /// The section body. May return a retained entity (`entity.into_any_element()`).
    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement;
}

#[derive(Default)]
struct Registry(Vec<Rc<dyn Section>>);

impl Global for Registry {}

/// Adds a section, replacing one with the same id.
pub fn register_section(cx: &mut App, section: Rc<dyn Section>) {
    if !cx.has_global::<Registry>() {
        cx.set_global(Registry::default());
    }
    let reg = &mut cx.global_mut::<Registry>().0;
    reg.retain(|s| s.id() != section.id());
    reg.push(section);
    cx.refresh_windows();
}

/// The contributed section for `id`, if any.
pub(crate) fn contributed(cx: &App, id: &str) -> Option<Rc<dyn Section>> {
    cx.try_global::<Registry>()
        .and_then(|r| r.0.iter().find(|s| s.id() == id).cloned())
}

/// Contributed sections that are not built-ins.
pub(crate) fn extra_sections(cx: &App) -> Vec<Rc<dyn Section>> {
    cx.try_global::<Registry>()
        .map(|r| {
            r.0.iter()
                .filter(|s| !model::SECTIONS.contains(&s.id()))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// A copy of the current settings.
#[must_use]
pub fn current(cx: &App) -> Option<AppSettings> {
    crate::shell::runtime::settings_handle(cx).map(|(s, _)| s.borrow().clone())
}

fn describe(e: &Invalid) -> String {
    match e {
        Invalid::Language => t!("settings.invalid.language").to_string(),
        Invalid::Url(why) => t!("settings.invalid.url", why = why).to_string(),
        Invalid::Shortcut(why) => t!("settings.invalid.shortcut", why = why).to_string(),
    }
}

/// Validates, applies (live where possible), persists. `Ok(true)` when a restart is needed for
/// the change to take effect. On `Err` nothing changed.
pub fn commit(cx: &mut App, change: Change) -> Result<bool, String> {
    let (settings, path) = crate::shell::runtime::settings_handle(cx)
        .ok_or_else(|| t!("settings.invalid.unavailable").to_string())?;
    let before = settings.borrow().clone();
    model::apply(&mut settings.borrow_mut(), &change).map_err(|e| describe(&e))?;
    if let Err(e) = perform(cx, &change, &settings.borrow()) {
        *settings.borrow_mut() = before;
        return Err(e);
    }
    if let Err(e) = settings.borrow().save(&path) {
        tracing::warn!("cannot save settings: {e}");
    }
    Ok(change.needs_restart())
}

fn perform(cx: &mut App, change: &Change, s: &AppSettings) -> Result<(), String> {
    use crate::shell::{autostart, runtime};
    match change {
        Change::Language(_) => {
            crate::i18n::apply(s.language.as_deref());
            runtime::language_changed(cx);
        }
        Change::ReduceMotion(v) => crate::os_prefs::set_user(cx, *v),
        Change::Autostart(on) => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let home = directories::BaseDirs::new()
                .map(|d| d.home_dir().to_path_buf())
                .ok_or_else(|| "no home directory".to_owned())?;
            let cfg = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from);
            let target = autostart::target_for_os(&home, cfg.as_deref());
            autostart::set_enabled(&target, *on, &exe).map_err(|e| e.to_string())?;
        }
        Change::KeepRunning(on) => runtime::set_keep_running(cx, *on),
        Change::PttShortcut(k) => runtime::rebind_shortcut(cx, k)?,
        Change::MiniEnabled(on) => runtime::set_mini_window(cx, *on),
        Change::PandoUrl(_)
        | Change::StartMinimized(_)
        | Change::MiniAutoHide(_)
        | Change::SystemNotifications(_)
        | Change::UpdateEnabled(_)
        | Change::UpdateUrl(_) => {}
    }
    Ok(())
}

/// Opens (or focuses) the settings window.
pub fn open(cx: &mut App) {
    crate::windows::open(
        cx,
        "settings",
        t!("settings.title").to_string(),
        (860.0, 640.0),
        |window, cx| {
            use crate::ui::AppContext as _;
            cx.new(|cx| SettingsView::new(window, cx))
        },
    );
}
