//! Optional autostart (PDA-T-0125): XDG autostart `.desktop` on Linux, a LaunchAgent on macOS and
//! the `HKCU\...\Run` value on Windows. The content builders are pure; `set_enabled` /
//! `is_enabled` do the I/O against an explicit [`Target`] so the Linux and macOS file logic is
//! testable anywhere.
//!
//! Pending: Flatpak must use the Background portal (`org.freedesktop.portal.Background`,
//! `autostart: true`) because `~/.config/autostart` is not writable there, and macOS 13+ may
//! prefer `SMAppService` over a LaunchAgent plist.

use std::path::{Path, PathBuf};

/// Where the autostart entry lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// `~/.config/autostart/dev.pando.assistant.desktop`.
    XdgFile(PathBuf),
    /// `~/Library/LaunchAgents/dev.pando.assistant.plist`.
    LaunchAgent(PathBuf),
    /// The current user's `Run` registry value.
    WindowsRun,
}

/// Desktop id / launchd label / registry value name.
pub const ENTRY_NAME: &str = "dev.pando.assistant";

/// Arguments the autostarted instance gets: stay in the tray.
pub const AUTOSTART_ARGS: &[&str] = &["--minimized"];

/// Escapes one argument for the `Exec=` line of a desktop entry.
fn exec_quote(arg: &str) -> String {
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./=:".contains(c))
    {
        arg.to_owned()
    } else {
        let esc: String = arg
            .chars()
            .flat_map(|c| match c {
                '"' | '`' | '$' | '\\' => vec!['\\', c],
                _ => vec![c],
            })
            .collect();
        format!("\"{esc}\"")
    }
}

/// The XDG autostart entry.
#[must_use]
pub fn desktop_entry(exe: &Path) -> String {
    let mut exec = vec![exec_quote(&exe.to_string_lossy())];
    exec.extend(AUTOSTART_ARGS.iter().map(|a| exec_quote(a)));
    format!(
        "[Desktop Entry]\nType=Application\nName=Pando Assistant\nComment=Voice assistant for Pando\n\
Exec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\nStartupNotify=false\n",
        exec.join(" ")
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The macOS LaunchAgent property list.
#[must_use]
pub fn launch_agent_plist(exe: &Path) -> String {
    let mut args = format!(
        "    <string>{}</string>\n",
        xml_escape(&exe.to_string_lossy())
    );
    for a in AUTOSTART_ARGS {
        args.push_str(&format!("    <string>{}</string>\n", xml_escape(a)));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>{ENTRY_NAME}</string>\n  <key>ProgramArguments</key>\n  <array>\n{args}  </array>\n  <key>RunAtLoad</key>\n  <true/>\n</dict>\n</plist>\n"
    )
}

/// The value written under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
#[must_use]
pub fn windows_run_value(exe: &Path) -> String {
    format!("\"{}\" {}", exe.to_string_lossy(), AUTOSTART_ARGS.join(" "))
}

/// The target of the running OS. `home` is the user's home, `config_home` the XDG config dir.
#[must_use]
pub fn target_for_os(home: &Path, config_home: Option<&Path>) -> Target {
    if cfg!(target_os = "windows") {
        Target::WindowsRun
    } else if cfg!(target_os = "macos") {
        Target::LaunchAgent(
            home.join("Library/LaunchAgents")
                .join(format!("{ENTRY_NAME}.plist")),
        )
    } else {
        let base = config_home.map_or_else(|| home.join(".config"), Path::to_path_buf);
        Target::XdgFile(base.join("autostart").join(format!("{ENTRY_NAME}.desktop")))
    }
}

/// Whether the entry exists.
#[must_use]
pub fn is_enabled(target: &Target) -> bool {
    match target {
        Target::XdgFile(p) | Target::LaunchAgent(p) => p.is_file(),
        Target::WindowsRun => windows_registry::query(),
    }
}

/// Creates or removes the entry.
pub fn set_enabled(target: &Target, enabled: bool, exe: &Path) -> std::io::Result<()> {
    match target {
        Target::XdgFile(p) | Target::LaunchAgent(p) => {
            if enabled {
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let body = if matches!(target, Target::XdgFile(_)) {
                    desktop_entry(exe)
                } else {
                    launch_agent_plist(exe)
                };
                std::fs::write(p, body)
            } else {
                match std::fs::remove_file(p) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                }
            }
        }
        Target::WindowsRun => windows_registry::set(enabled, exe),
    }
}

mod windows_registry {
    use std::path::Path;

    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    #[cfg(windows)]
    pub fn query() -> bool {
        std::process::Command::new("reg")
            .args(["query", KEY, "/v", super::ENTRY_NAME])
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[cfg(windows)]
    pub fn set(enabled: bool, exe: &Path) -> std::io::Result<()> {
        let out = if enabled {
            std::process::Command::new("reg")
                .args(["add", KEY, "/v", super::ENTRY_NAME, "/t", "REG_SZ", "/d"])
                .arg(super::windows_run_value(exe))
                .arg("/f")
                .output()?
        } else {
            std::process::Command::new("reg")
                .args(["delete", KEY, "/v", super::ENTRY_NAME, "/f"])
                .output()?
        };
        // Deleting a value that does not exist is fine.
        if out.status.success() || !enabled {
            Ok(())
        } else {
            Err(std::io::Error::other(
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ))
        }
    }

    #[cfg(not(windows))]
    pub fn query() -> bool {
        let _ = KEY;
        false
    }

    #[cfg(not(windows))]
    pub fn set(_enabled: bool, _exe: &Path) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "the Windows registry target only exists on Windows",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_quotes_and_minimizes() {
        let e = desktop_entry(Path::new("/opt/Pando Assistant/pando-assistant"));
        assert!(
            e.contains("Exec=\"/opt/Pando Assistant/pando-assistant\" --minimized\n"),
            "{e}"
        );
        assert!(
            desktop_entry(Path::new("/usr/bin/pando-assistant"))
                .contains("Exec=/usr/bin/pando-assistant --minimized")
        );
    }

    #[test]
    fn plist_and_registry_values() {
        let p = launch_agent_plist(Path::new("/Applications/Pando & Co/pando"));
        assert!(p.contains("<string>/Applications/Pando &amp; Co/pando</string>"));
        assert!(p.contains("<key>RunAtLoad</key>") && p.contains("--minimized"));
        assert_eq!(
            windows_run_value(Path::new(r"C:\Program Files\Pando\pando-assistant.exe")),
            r#""C:\Program Files\Pando\pando-assistant.exe" --minimized"#
        );
    }

    #[test]
    fn toggle_creates_and_removes_the_file_entry() {
        let home = tempfile::tempdir().expect("home");
        let target = Target::XdgFile(
            home.path()
                .join(".config/autostart/dev.pando.assistant.desktop"),
        );
        assert!(!is_enabled(&target));
        set_enabled(&target, true, Path::new("/usr/bin/pando-assistant")).expect("enable");
        assert!(is_enabled(&target));
        set_enabled(&target, true, Path::new("/usr/bin/pando-assistant")).expect("idempotent");
        set_enabled(&target, false, Path::new("/usr/bin/pando-assistant")).expect("disable");
        assert!(!is_enabled(&target));
        set_enabled(&target, false, Path::new("x")).expect("removing a missing entry is fine");
        let agent = Target::LaunchAgent(
            home.path()
                .join("Library/LaunchAgents/dev.pando.assistant.plist"),
        );
        set_enabled(&agent, true, Path::new("/Applications/Pando")).expect("plist");
        assert!(is_enabled(&agent));
    }

    #[test]
    fn target_follows_the_os_layout() {
        let t = target_for_os(Path::new("/home/u"), None);
        if cfg!(target_os = "linux") {
            assert_eq!(
                t,
                Target::XdgFile(PathBuf::from(
                    "/home/u/.config/autostart/dev.pando.assistant.desktop"
                ))
            );
            let t = target_for_os(Path::new("/home/u"), Some(Path::new("/cfg")));
            assert_eq!(
                t,
                Target::XdgFile(PathBuf::from("/cfg/autostart/dev.pando.assistant.desktop"))
            );
        }
    }
}
