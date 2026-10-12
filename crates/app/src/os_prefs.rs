//! Operating-system accessibility preferences (PDA-T-0129): reduced motion on GNOME / KDE,
//! macOS and Windows, polled in the background and merged with the user's own setting into
//! `cx.set_reduce_motion` so the organism follows it live.
//!
//! Sources: GNOME `org.gnome.desktop.interface enable-animations`, KDE
//! `AnimationDurationFactor` (0 = off), macOS `com.apple.universalaccess reduceMotion`
//! (`accessibilityDisplayShouldReduceMotion`), Windows `SPI_GETCLIENTAREAANIMATION`.

use std::process::{Command, Stdio};
use std::time::Duration;

use crate::ui::{App, Global};

/// How often the OS preference is re-read (none of the platforms offers a portable change event).
pub const POLL: Duration = Duration::from_secs(5);

/// `gsettings get ... enable-animations` output: `true` / `false` -> reduce motion when false.
#[must_use]
pub fn reduce_from_gsettings(out: &str) -> Option<bool> {
    match out.trim() {
        "true" => Some(false),
        "false" => Some(true),
        _ => None,
    }
}

/// KDE `AnimationDurationFactor`: `0` (or `0.0`) means animations are off.
#[must_use]
pub fn reduce_from_kde_factor(out: &str) -> Option<bool> {
    out.trim().parse::<f64>().ok().map(|f| f <= 0.0)
}

/// macOS `defaults read com.apple.universalaccess reduceMotion`: `1` = reduce.
#[must_use]
pub fn reduce_from_macos_defaults(out: &str) -> Option<bool> {
    match out.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// PowerShell output of `SPI_GETCLIENTAREAANIMATION`: `True` = animations on.
#[must_use]
pub fn reduce_from_windows_animation(out: &str) -> Option<bool> {
    match out.trim().to_ascii_lowercase().as_str() {
        "true" => Some(false),
        "false" => Some(true),
        _ => None,
    }
}

/// The effective setting: the user's choice or the OS's.
#[must_use]
pub fn effective(user: bool, os: Option<bool>) -> bool {
    user || os.unwrap_or(false)
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Reads the OS preference (blocking: call off the UI thread). `None` = unknown.
#[must_use]
pub fn os_reduce_motion() -> Option<bool> {
    if cfg!(target_os = "macos") {
        // A missing key means the default (off).
        return Some(
            run(
                "defaults",
                &["read", "com.apple.universalaccess", "reduceMotion"],
            )
            .and_then(|o| reduce_from_macos_defaults(&o))
            .unwrap_or(false),
        );
    }
    if cfg!(target_os = "windows") {
        let script = "Add-Type -MemberDefinition '[DllImport(\"user32.dll\")] public static extern bool SystemParametersInfo(int a,int b,ref bool c,int d);' -Name N -Namespace W; $v=$false; [void][W.N]::SystemParametersInfo(0x1042,0,[ref]$v,0); $v";
        return run(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", script],
        )
        .and_then(|o| reduce_from_windows_animation(&o));
    }
    run(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "enable-animations"],
    )
    .and_then(|o| reduce_from_gsettings(&o))
    .or_else(|| {
        run(
            "kreadconfig6",
            &[
                "--file",
                "kdeglobals",
                "--group",
                "KDE",
                "--key",
                "AnimationDurationFactor",
            ],
        )
        .and_then(|o| reduce_from_kde_factor(&o))
    })
}

/// Current user choice and OS state.
#[derive(Debug, Default)]
struct Prefs {
    user: bool,
    os: Option<bool>,
}

impl Global for Prefs {}

fn apply(cx: &mut App) {
    let p = cx.global::<Prefs>();
    let on = effective(p.user, p.os);
    if cx.reduce_motion() != on {
        cx.set_reduce_motion(on);
        cx.refresh_windows();
    }
}

/// Starts following the OS preference; `user` is the app's own setting.
pub fn install(cx: &mut App, user: bool) {
    cx.set_global(Prefs { user, os: None });
    apply(cx);
    cx.spawn(async move |cx| {
        loop {
            let os = cx
                .background_executor()
                .spawn(async { os_reduce_motion() })
                .await;
            cx.update(|cx| {
                cx.global_mut::<Prefs>().os = os;
                apply(cx);
            });
            cx.background_executor().timer(POLL).await;
        }
    })
    .detach();
}

/// The user changed the setting.
pub fn set_user(cx: &mut App, user: bool) {
    if cx.has_global::<Prefs>() {
        cx.global_mut::<Prefs>().user = user;
        apply(cx);
    } else {
        cx.set_reduce_motion(user);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers() {
        assert_eq!(reduce_from_gsettings("true\n"), Some(false));
        assert_eq!(reduce_from_gsettings("false\n"), Some(true));
        assert_eq!(reduce_from_gsettings("oops"), None);
        assert_eq!(reduce_from_kde_factor("0\n"), Some(true));
        assert_eq!(reduce_from_kde_factor("1.0"), Some(false));
        assert_eq!(reduce_from_macos_defaults("1\n"), Some(true));
        assert_eq!(reduce_from_macos_defaults("0"), Some(false));
        assert_eq!(reduce_from_windows_animation("True\r\n"), Some(false));
        assert_eq!(reduce_from_windows_animation("False"), Some(true));
    }

    #[test]
    fn user_or_os_wins() {
        assert!(effective(true, Some(false)));
        assert!(effective(false, Some(true)));
        assert!(!effective(false, None));
        assert!(!effective(false, Some(false)));
    }
}
