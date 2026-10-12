//! Hold-to-talk on Wayland through the XDG `GlobalShortcuts` portal (PDA-T-0121) plus the
//! session / fallback helpers.
//!
//! `ashpd` and `ksni` run with `async-io` on a worker thread driven by
//! `futures_lite::future::block_on` (T-0026: ashpd refuses tokio next to GPUI). The thread only
//! sends [`Msg`]s into the shell inbox. Where the portal is missing (Sway, Hyprland, COSMIC,
//! GNOME < 48) the status says so and the settings show [`fallback_lines`] for the compositor
//! config, which use `pando-assistant ptt start|stop` over the IPC socket.

use super::ipc::PttAction;

/// Kind of display session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    /// X11 (also macOS / Windows count as "not Wayland").
    Other,
    /// A Wayland session: global grabs need the portal.
    Wayland,
}

/// Detects the session from `XDG_SESSION_TYPE` / `WAYLAND_DISPLAY` values.
#[must_use]
pub fn session_from_env(xdg_session_type: Option<&str>, wayland_display: Option<&str>) -> Session {
    match xdg_session_type {
        Some(t) if t.eq_ignore_ascii_case("wayland") => Session::Wayland,
        Some(t) if t.eq_ignore_ascii_case("x11") => Session::Other,
        _ if wayland_display.is_some_and(|d| !d.is_empty()) => Session::Wayland,
        _ => Session::Other,
    }
}

/// Session of this process.
#[must_use]
pub fn current_session() -> Session {
    session_from_env(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )
}

/// Converts the settings syntax (`ctrl+alt+space`) into the portal's preferred-trigger syntax
/// (`CTRL+ALT+space`: upper-case modifiers, key as an XKB name).
#[must_use]
pub fn portal_trigger(shortcut: &str) -> String {
    let parts: Vec<&str> = shortcut
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let Some((key, mods)) = parts.split_last() else {
        return String::new();
    };
    let mut out: Vec<String> = mods
        .iter()
        .map(|m| match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "CTRL".to_owned(),
            "alt" | "option" => "ALT".to_owned(),
            "shift" => "SHIFT".to_owned(),
            "super" | "meta" | "cmd" | "command" | "logo" => "LOGO".to_owned(),
            other => other.to_ascii_uppercase(),
        })
        .collect();
    let key = key
        .strip_prefix("Key")
        .or_else(|| key.strip_prefix("key"))
        .unwrap_or(key);
    out.push(key.to_ascii_lowercase());
    out.join("+")
}

/// Lines for a compositor config that drive hold-to-talk through the CLI.
#[must_use]
pub fn fallback_lines(compositor: &str, shortcut: &str) -> Vec<String> {
    let exe = "pando-assistant";
    let mods_key = |mod_name: &str| -> String {
        let parts: Vec<String> = shortcut
            .split('+')
            .map(|p| {
                let l = p.trim().to_ascii_lowercase();
                match l.as_str() {
                    "ctrl" | "control" => "Ctrl".to_owned(),
                    "alt" => mod_name.to_owned(),
                    other => other.to_owned(),
                }
            })
            .collect();
        parts.join("+")
    };
    match compositor.to_ascii_lowercase().as_str() {
        "sway" => {
            let k = mods_key("Mod1");
            vec![
                format!("bindsym --no-repeat {k} exec {exe} ptt start"),
                format!("bindsym --release {k} exec {exe} ptt stop"),
            ]
        }
        "niri" => {
            let k = mods_key("Alt");
            vec![format!(
                "binds {{ {k} allow-when-locked=true repeat=false {{ spawn \"{exe}\" \"ptt\" \"toggle\"; }} }}"
            )]
        }
        "hyprland" => {
            let k = mods_key("ALT");
            let (m, key) = k.rsplit_once('+').unwrap_or(("", &k));
            vec![
                format!(
                    "bind = {}, {key}, exec, {exe} ptt start",
                    m.replace('+', " ")
                ),
                format!(
                    "bindr = {}, {key}, exec, {exe} ptt stop",
                    m.replace('+', " ")
                ),
            ]
        }
        _ => vec![
            format!("press:   {exe} ptt start"),
            format!("release: {exe} ptt stop"),
            format!("(or one key that toggles: {exe} ptt toggle)"),
        ],
    }
}

/// Maps a portal activation to the IPC action.
#[must_use]
pub fn action_for(activated: bool) -> PttAction {
    if activated {
        PttAction::Start
    } else {
        PttAction::Stop
    }
}

#[cfg(target_os = "linux")]
pub use backend::spawn;

#[cfg(target_os = "linux")]
mod backend {
    use async_channel::Sender;

    use super::{action_for, portal_trigger};
    use crate::shell::controller::Msg;
    use crate::shell::ipc::Command;

    fn status(tx: &Sender<Msg>, text: impl Into<String>) {
        let _ = tx.try_send(Msg::Backend {
            name: "portal",
            status: text.into(),
        });
    }

    /// Starts the portal listener on its own thread. `shortcut` is only the *preferred*
    /// trigger: the user (or the desktop) has the last word, and the portal remembers it.
    pub fn spawn(shortcut: &str, tx: Sender<Msg>) {
        let trigger = portal_trigger(shortcut);
        let _ = std::thread::Builder::new()
            .name("pando-portal".into())
            .spawn(move || futures_lite::future::block_on(run(trigger, tx)));
    }

    async fn run(trigger: String, tx: Sender<Msg>) {
        use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
        use futures_util::StreamExt as _;
        let portal = match GlobalShortcuts::new().await {
            Ok(p) => p,
            Err(e) => {
                status(&tx, format!("unavailable: {e}"));
                return;
            }
        };
        let session = match portal.create_session(Default::default()).await {
            Ok(s) => s,
            Err(e) => {
                status(&tx, format!("unavailable: create_session: {e}"));
                return;
            }
        };
        let shortcuts = [
            NewShortcut::new("ptt", "Hold to talk to Pando").preferred_trigger(trigger.as_str())
        ];
        if let Err(e) = portal
            .bind_shortcuts(&session, &shortcuts, None, Default::default())
            .await
        {
            status(&tx, format!("unavailable: bind_shortcuts: {e}"));
            return;
        }
        status(&tx, "bound");
        let (Ok(act), Ok(deact)) = (
            portal.receive_activated().await,
            portal.receive_deactivated().await,
        ) else {
            status(&tx, "unavailable: no activation signals");
            return;
        };
        let mut events = futures_util::stream::select(act.map(|_| true), deact.map(|_| false));
        while let Some(pressed) = events.next().await {
            let _ = tx
                .send(Msg::Command(Command::Ptt {
                    action: action_for(pressed),
                }))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_detection() {
        assert_eq!(session_from_env(Some("wayland"), None), Session::Wayland);
        assert_eq!(
            session_from_env(Some("x11"), Some("wayland-0")),
            Session::Other
        );
        assert_eq!(session_from_env(None, Some("wayland-0")), Session::Wayland);
        assert_eq!(session_from_env(None, None), Session::Other);
    }

    #[test]
    fn triggers_use_the_portal_syntax() {
        assert_eq!(portal_trigger("ctrl+alt+space"), "CTRL+ALT+space");
        assert_eq!(portal_trigger("Ctrl+Shift+KeyP"), "CTRL+SHIFT+p");
        assert_eq!(portal_trigger("super+f9"), "LOGO+f9");
        assert_eq!(portal_trigger(""), "");
    }

    #[test]
    fn compositor_fallbacks_press_and_release() {
        let sway = fallback_lines("sway", "ctrl+alt+space");
        assert_eq!(
            sway[0],
            "bindsym --no-repeat Ctrl+Mod1+space exec pando-assistant ptt start"
        );
        assert!(
            sway[1].starts_with("bindsym --release Ctrl+Mod1+space")
                && sway[1].ends_with("ptt stop")
        );
        let hypr = fallback_lines("Hyprland", "ctrl+alt+space");
        assert!(hypr[0].starts_with("bind = Ctrl ALT, space,") && hypr[1].starts_with("bindr ="));
        assert_eq!(fallback_lines("niri", "ctrl+alt+space").len(), 1);
        assert_eq!(fallback_lines("other", "x").len(), 3);
        assert_eq!(action_for(true), PttAction::Start);
    }
}
