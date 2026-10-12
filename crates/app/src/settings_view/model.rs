//! Pure settings model (PDA-T-0126): every user-editable setting as a [`Change`], validated and
//! applied to [`AppSettings`]; [`Change::needs_restart`] marks the few that cannot apply live.
//! Secrets (remote credentials) never pass through here: they go to the OS keychain.

use crate::i18n::LANGUAGES;
use crate::settings::AppSettings;

/// One edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// UI language tag, `None` = follow the system.
    Language(Option<String>),
    /// Manual Pando URL, `None` = discover.
    PandoUrl(Option<String>),
    /// Reduce motion (the OS preference is merged on top).
    ReduceMotion(bool),
    /// Start with the session.
    Autostart(bool),
    /// Keep running in the tray when the window closes.
    KeepRunning(bool),
    /// Start hidden in the tray.
    StartMinimized(bool),
    /// Hold-to-talk shortcut (`global-hotkey` syntax).
    PttShortcut(String),
    /// Show the mini window.
    MiniEnabled(bool),
    /// Hide the mini window while the main one has focus.
    MiniAutoHide(bool),
    /// OS notifications while unfocused.
    SystemNotifications(bool),
    /// Background update checks.
    UpdateEnabled(bool),
    /// Update feed URL.
    UpdateUrl(Option<String>),
}

impl Change {
    /// The change only takes effect after a restart.
    #[must_use]
    pub fn needs_restart(&self) -> bool {
        matches!(
            self,
            Self::PandoUrl(_)
                | Self::StartMinimized(_)
                | Self::UpdateEnabled(_)
                | Self::UpdateUrl(_)
        )
    }
}

/// Why a change was refused (already localized by the caller if shown).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invalid {
    /// Unknown language tag.
    Language,
    /// Not a valid local / http(s) URL.
    Url(String),
    /// Not a valid shortcut.
    Shortcut(String),
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// Validates and applies `change` to `s`.
pub fn apply(s: &mut AppSettings, change: &Change) -> Result<(), Invalid> {
    match change {
        Change::Language(tag) => {
            if let Some(t) = tag
                && !LANGUAGES.iter().any(|(l, _)| l == t)
            {
                return Err(Invalid::Language);
            }
            s.language.clone_from(tag);
        }
        Change::PandoUrl(url) => {
            let url = clean(url);
            if let Some(u) = &url {
                pando_client::discovery::validate_url(u, false)
                    .map_err(|e| Invalid::Url(e.to_string()))?;
            }
            s.pando_url = url;
        }
        Change::ReduceMotion(v) => s.reduce_motion = *v,
        Change::Autostart(v) => s.shell.autostart = *v,
        Change::KeepRunning(v) => s.shell.keep_running = *v,
        Change::StartMinimized(v) => s.shell.start_minimized = *v,
        Change::PttShortcut(k) => {
            crate::shell::hotkey::parse(k).map_err(Invalid::Shortcut)?;
            s.shell.ptt_shortcut = k.trim().to_owned();
        }
        Change::MiniEnabled(v) => s.shell.mini_enabled = *v,
        Change::MiniAutoHide(v) => s.shell.mini_auto_hide = *v,
        Change::SystemNotifications(v) => s.shell.system_notifications = *v,
        Change::UpdateEnabled(v) => s.update.enabled = *v,
        Change::UpdateUrl(url) => {
            let url = clean(url);
            if let Some(u) = &url
                && !(u.starts_with("https://") || u.starts_with("http://"))
            {
                return Err(Invalid::Url("the feed must be http(s)".into()));
            }
            s.update.url = url;
        }
    }
    Ok(())
}

/// Ids of the built-in sections, in display order.
pub const SECTIONS: [&str; 7] = [
    "connection",
    "voice",
    "audio",
    "shortcuts",
    "appearance",
    "system",
    "advanced",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_changes_apply() {
        let mut s = AppSettings::default();
        apply(&mut s, &Change::Language(Some("en".into()))).expect("lang");
        apply(&mut s, &Change::Language(None)).expect("auto");
        apply(
            &mut s,
            &Change::PandoUrl(Some(" http://127.0.0.1:8080 ".into())),
        )
        .expect("url");
        assert_eq!(s.pando_url.as_deref(), Some("http://127.0.0.1:8080"));
        apply(&mut s, &Change::PandoUrl(Some("  ".into()))).expect("clear");
        assert_eq!(s.pando_url, None);
        apply(&mut s, &Change::PttShortcut("ctrl+shift+KeyP".into())).expect("shortcut");
        apply(&mut s, &Change::Autostart(true)).expect("autostart");
        apply(&mut s, &Change::KeepRunning(false)).expect("keep");
        apply(
            &mut s,
            &Change::UpdateUrl(Some("https://u.example/p".into())),
        )
        .expect("feed");
        assert!(s.shell.autostart && !s.shell.keep_running);
        assert_eq!(s.shell.ptt_shortcut, "ctrl+shift+KeyP");
        assert_eq!(s.update.url.as_deref(), Some("https://u.example/p"));
    }

    #[test]
    fn invalid_changes_leave_the_settings_untouched() {
        let mut s = AppSettings::default();
        let before = s.clone();
        assert_eq!(
            apply(&mut s, &Change::Language(Some("xx".into()))),
            Err(Invalid::Language)
        );
        assert!(matches!(
            apply(&mut s, &Change::PandoUrl(Some("ftp://x".into()))),
            Err(Invalid::Url(_))
        ));
        assert!(
            matches!(
                apply(
                    &mut s,
                    &Change::PandoUrl(Some("https://example.com".into()))
                ),
                Err(Invalid::Url(_))
            ),
            "remote hosts are refused"
        );
        assert!(matches!(
            apply(&mut s, &Change::PttShortcut("nope nope".into())),
            Err(Invalid::Shortcut(_))
        ));
        assert!(matches!(
            apply(&mut s, &Change::UpdateUrl(Some("file:///x".into()))),
            Err(Invalid::Url(_))
        ));
        assert_eq!(s, before);
    }

    #[test]
    fn restart_marks() {
        assert!(Change::PandoUrl(None).needs_restart());
        assert!(!Change::Language(None).needs_restart());
        assert!(!Change::ReduceMotion(true).needs_restart());
        assert!(!Change::PttShortcut("a".into()).needs_restart());
    }
}
