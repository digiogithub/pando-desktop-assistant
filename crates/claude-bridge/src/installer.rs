//! Claude Code hooks installer / uninstaller (PDA-T-0131).
//!
//! Merges one hook handler per [`HookKind`] into a Claude Code `settings.json`
//! (`{"hooks": {"<Event>": [{"hooks": [{"type":"command","command":"<forwarder>"}]}]}}`,
//! schema: <https://code.claude.com/docs/en/hooks>). Guarantees:
//!
//! * idempotent: our handler is recognised by its command and never duplicated;
//! * unrelated user config (other hooks, other keys, key order) is preserved;
//! * a timestamped backup is written before every change to an existing file;
//! * the write is atomic (temp file + rename) and follows symlinks (dotfile setups);
//! * an unparsable settings file is never touched.
//!
//! The installer takes explicit paths, so tests (and callers) never need `~/.claude`.

use crate::events::HookKind;
use crate::runtime::RuntimeFiles;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// File name of the forwarder script (also the marker identifying our handlers).
pub const FORWARDER_NAME: &str = "pando-claude-hook.sh";
/// Hook timeout in seconds registered in settings (the script itself caps at 2 s).
const HOOK_TIMEOUT_S: u64 = 5;

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0} is not valid JSON; refusing to modify it: {1}")]
    InvalidJson(PathBuf, serde_json::Error),
    #[error("{0}: top-level value, \"hooks\" or an event entry has an unexpected shape")]
    UnexpectedShape(PathBuf),
}

/// Outcome of install/uninstall.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstallReport {
    /// `false` when the file already was in the requested state.
    pub changed: bool,
    /// Backup of the previous file, when one was made.
    pub backup: Option<PathBuf>,
}

/// Result of [`HookInstaller::status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallStatus {
    /// Events that have our handler registered.
    pub registered: Vec<HookKind>,
    /// Events that are missing it.
    pub missing: Vec<HookKind>,
    /// The forwarder script exists and is executable.
    pub script_ok: bool,
}

impl InstallStatus {
    pub fn is_installed(&self) -> bool {
        self.missing.is_empty() && self.script_ok
    }
}

/// Installs hooks into one settings file using one forwarder script.
#[derive(Debug, Clone)]
pub struct HookInstaller {
    settings: PathBuf,
    script: PathBuf,
}

impl HookInstaller {
    /// `settings`: e.g. `~/.claude/settings.json`; `script_dir`: where the forwarder lives
    /// (the app's data dir).
    pub fn new(settings: impl Into<PathBuf>, script_dir: impl AsRef<Path>) -> Self {
        Self {
            settings: settings.into(),
            script: script_dir.as_ref().join(FORWARDER_NAME),
        }
    }

    /// `$HOME/.claude/settings.json`.
    pub fn default_settings_path() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude/settings.json"))
    }

    pub fn script_path(&self) -> &Path {
        &self.script
    }

    /// Write the forwarder script (0700) and merge the hooks into settings.
    pub fn install(&self, runtime: &RuntimeFiles) -> Result<InstallReport, InstallError> {
        self.write_script(runtime)?;
        self.edit(|root| {
            let cmd = self.command();
            let hooks = hooks_map(root, &self.settings)?;
            let mut changed = false;
            for kind in HookKind::ALL {
                let groups = event_groups(hooks, kind.name(), &self.settings)?;
                changed |= ensure_handler(groups, &cmd);
            }
            Ok(changed)
        })
    }

    /// Remove only our handlers (and the groups/events that become empty because of it).
    pub fn uninstall(&self) -> Result<InstallReport, InstallError> {
        let report = self.edit(|root| {
            let Some(hooks) = root.get_mut("hooks") else {
                return Ok(false);
            };
            let hooks = hooks
                .as_object_mut()
                .ok_or_else(|| InstallError::UnexpectedShape(self.settings.clone()))?;
            let mut changed = false;
            let events: Vec<String> = hooks.keys().cloned().collect();
            for name in events {
                let Some(Value::Array(groups)) = hooks.get_mut(&name) else {
                    continue;
                };
                changed |= remove_handler(groups);
                if groups.is_empty() {
                    hooks.remove(&name);
                }
            }
            if hooks.is_empty() && changed {
                root.remove("hooks");
            }
            Ok(changed)
        })?;
        let _ = std::fs::remove_file(&self.script);
        Ok(report)
    }

    /// Verify what is registered without writing anything.
    pub fn status(&self) -> Result<InstallStatus, InstallError> {
        let root = self.read()?.unwrap_or_else(|| Value::Object(Map::new()));
        let mut registered = Vec::new();
        let mut missing = Vec::new();
        for kind in HookKind::ALL {
            let has = root
                .get("hooks")
                .and_then(|h| h.get(kind.name()))
                .and_then(Value::as_array)
                .is_some_and(|g| g.iter().any(|grp| group_handlers(grp).any(is_ours)));
            if has {
                registered.push(kind);
            } else {
                missing.push(kind);
            }
        }
        Ok(InstallStatus {
            registered,
            missing,
            script_ok: script_executable(&self.script),
        })
    }

    fn command(&self) -> String {
        shell_quote(&self.script.to_string_lossy())
    }

    fn io(path: &Path) -> impl FnOnce(std::io::Error) -> InstallError + '_ {
        move |source| InstallError::Io {
            path: path.to_owned(),
            source,
        }
    }

    fn read(&self) -> Result<Option<Value>, InstallError> {
        match std::fs::read_to_string(&self.settings) {
            Ok(s) if s.trim().is_empty() => Ok(None),
            Ok(s) => serde_json::from_str(&s)
                .map(Some)
                .map_err(|e| InstallError::InvalidJson(self.settings.clone(), e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Self::io(&self.settings)(e)),
        }
    }

    /// Read-modify-write with backup; `f` returns whether it changed anything.
    fn edit(
        &self,
        f: impl FnOnce(&mut Map<String, Value>) -> Result<bool, InstallError>,
    ) -> Result<InstallReport, InstallError> {
        let existed = self.settings.exists();
        let mut root = self.read()?.unwrap_or_else(|| Value::Object(Map::new()));
        let obj = root
            .as_object_mut()
            .ok_or_else(|| InstallError::UnexpectedShape(self.settings.clone()))?;
        if !f(obj)? {
            return Ok(InstallReport::default());
        }
        // Follow symlinks so a dotfiles-managed settings.json stays a symlink.
        let target =
            std::fs::canonicalize(&self.settings).unwrap_or_else(|_| self.settings.clone());
        let mut backup = None;
        if existed {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let mut name = target.as_os_str().to_owned();
            name.push(format!(".pando-backup-{secs}"));
            let b = PathBuf::from(name);
            std::fs::copy(&target, &b).map_err(Self::io(&b))?;
            backup = Some(b);
        } else if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(Self::io(parent))?;
        }
        let mut text = serde_json::to_string_pretty(&root)
            .map_err(|e| InstallError::InvalidJson(self.settings.clone(), e))?;
        text.push('\n');
        let mut tmp_name = target.as_os_str().to_owned();
        tmp_name.push(".pando-tmp");
        let tmp = PathBuf::from(tmp_name);
        std::fs::write(&tmp, text).map_err(Self::io(&tmp))?;
        if existed && let Ok(meta) = std::fs::metadata(&target) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        std::fs::rename(&tmp, &target).map_err(Self::io(&target))?;
        Ok(InstallReport {
            changed: true,
            backup,
        })
    }

    fn write_script(&self, runtime: &RuntimeFiles) -> Result<(), InstallError> {
        let script = forwarder_script(runtime);
        if let Some(parent) = self.script.parent() {
            std::fs::create_dir_all(parent).map_err(Self::io(parent))?;
        }
        if std::fs::read_to_string(&self.script).is_ok_and(|cur| cur == script)
            && script_executable(&self.script)
        {
            return Ok(());
        }
        std::fs::write(&self.script, script).map_err(Self::io(&self.script))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.script, std::fs::Permissions::from_mode(0o700))
                .map_err(Self::io(&self.script))?;
        }
        Ok(())
    }
}

/// POSIX forwarder: reads the port/secret files, posts stdin with `curl`, always exits 0 and
/// returns within ~2 s (connect 1 s) when the assistant is not running. The secret is passed
/// through `curl -H @file` so it never shows up in `ps`.
pub fn forwarder_script(runtime: &RuntimeFiles) -> String {
    let port = shell_quote(&runtime.port_file().to_string_lossy());
    let headers = shell_quote(&runtime.headers_file().to_string_lossy());
    format!(
        r#"#!/bin/sh
# Pando Desktop Assistant: forwards Claude Code hook events to the local assistant.
# Generated by the installer; safe to delete (uninstall removes it). Always exits 0.
PORT_FILE={port}
HEADERS_FILE={headers}
[ -r "$PORT_FILE" ] && [ -r "$HEADERS_FILE" ] || exit 0
command -v curl >/dev/null 2>&1 || exit 0
port=$(sed -n 's/^port=//p' "$PORT_FILE" 2>/dev/null | head -n 1)
[ -n "$port" ] || exit 0
curl -s -o /dev/null --connect-timeout 1 --max-time 2 -X POST \
  -H "Content-Type: application/json" \
  -H @"$HEADERS_FILE" \
  ${{CLAUDE_CODE_MESSAGING_SOCKET:+-H "X-Claude-Msg-Socket: $CLAUDE_CODE_MESSAGING_SOCKET"}} \
  ${{CLAUDE_CODE_MESSAGING_TOKEN:+-H "X-Claude-Msg-Token: $CLAUDE_CODE_MESSAGING_TOKEN"}} \
  --data-binary @- "http://127.0.0.1:$port/claude/hook" >/dev/null 2>&1
exit 0
"#
    )
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn script_executable(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o100 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

fn hooks_map<'a>(
    root: &'a mut Map<String, Value>,
    path: &Path,
) -> Result<&'a mut Map<String, Value>, InstallError> {
    root.entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| InstallError::UnexpectedShape(path.to_owned()))
}

fn event_groups<'a>(
    hooks: &'a mut Map<String, Value>,
    event: &str,
    path: &Path,
) -> Result<&'a mut Vec<Value>, InstallError> {
    hooks
        .entry(event)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| InstallError::UnexpectedShape(path.to_owned()))
}

fn group_handlers(group: &Value) -> impl Iterator<Item = &Value> {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// A handler is ours when its command runs a script named [`FORWARDER_NAME`].
fn is_ours(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains(FORWARDER_NAME))
}

/// Make sure exactly our handler (with `cmd`) exists; returns whether `groups` changed.
fn ensure_handler(groups: &mut Vec<Value>, cmd: &str) -> bool {
    let mut found = false;
    let mut changed = false;
    for group in groups.iter_mut() {
        let Some(Value::Array(handlers)) = group.get_mut("hooks") else {
            continue;
        };
        for h in handlers.iter_mut().filter(|h| is_ours(h)) {
            if found {
                // Duplicate from an earlier buggy install: handled below by removal.
                continue;
            }
            found = true;
            if h.get("command").and_then(Value::as_str) != Some(cmd) {
                h["command"] = json!(cmd);
                changed = true;
            }
        }
    }
    if found {
        // Drop duplicates beyond the first.
        let mut seen = false;
        for group in groups.iter_mut() {
            if let Some(Value::Array(handlers)) = group.get_mut("hooks") {
                let before = handlers.len();
                handlers.retain(|h| {
                    if is_ours(h) {
                        if seen {
                            return false;
                        }
                        seen = true;
                    }
                    true
                });
                changed |= handlers.len() != before;
            }
        }
        let before = groups.len();
        groups.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|h| !h.is_empty())
        });
        return changed || groups.len() != before;
    }
    groups.push(json!({
        "hooks": [{ "type": "command", "command": cmd, "timeout": HOOK_TIMEOUT_S }]
    }));
    true
}

/// Remove our handlers; groups left empty *by us* are removed too.
fn remove_handler(groups: &mut Vec<Value>) -> bool {
    let mut changed = false;
    for group in groups.iter_mut() {
        if let Some(Value::Array(handlers)) = group.get_mut("hooks") {
            let before = handlers.len();
            handlers.retain(|h| !is_ours(h));
            changed |= handlers.len() != before;
        }
    }
    if changed {
        groups.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|h| !h.is_empty())
        });
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fx {
        _tmp: tempfile::TempDir,
        settings: PathBuf,
        rt: RuntimeFiles,
        inst: HookInstaller,
    }

    fn fx(initial: Option<&str>) -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let settings = tmp.path().join("claude/settings.json");
        if let Some(s) = initial {
            std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
            std::fs::write(&settings, s).unwrap();
        }
        let rt = RuntimeFiles::new(tmp.path().join("rt"));
        let inst = HookInstaller::new(&settings, tmp.path().join("bin"));
        Fx {
            _tmp: tmp,
            settings,
            rt,
            inst,
        }
    }

    fn load(p: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    const USER: &str = r#"{
  "model": "opus",
  "permissions": {"allow": ["Bash(ls)"]},
  "hooks": {
    "Stop": [{"matcher": "", "hooks": [{"type": "command", "command": "notify-send done"}]}],
    "PostToolUse": [{"matcher": "Edit", "hooks": [{"type": "command", "command": "fmt.sh"}]}]
  }
}"#;

    #[test]
    fn install_is_idempotent_and_preserves_user_config() {
        let f = fx(Some(USER));
        let r1 = f.inst.install(&f.rt).unwrap();
        assert!(r1.changed);
        assert!(r1.backup.as_ref().unwrap().exists());
        let after1 = load(&f.settings);
        assert_eq!(after1["model"], "opus");
        assert_eq!(after1["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(
            after1["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            "fmt.sh"
        );
        assert_eq!(
            after1["hooks"]["Stop"][0]["hooks"][0]["command"],
            "notify-send done"
        );
        assert_eq!(after1["hooks"]["Stop"].as_array().unwrap().len(), 2);
        let st = f.inst.status().unwrap();
        assert!(st.is_installed(), "{st:?}");

        let r2 = f.inst.install(&f.rt).unwrap();
        assert!(!r2.changed && r2.backup.is_none());
        assert_eq!(
            load(&f.settings),
            after1,
            "second install must not change anything"
        );
    }

    #[test]
    fn uninstall_restores_previous_hooks() {
        let f = fx(Some(USER));
        let before = load(&f.settings);
        f.inst.install(&f.rt).unwrap();
        let r = f.inst.uninstall().unwrap();
        assert!(r.changed);
        assert_eq!(load(&f.settings), before);
        assert!(!f.inst.status().unwrap().is_installed());
        assert!(!f.inst.script_path().exists());
        assert!(!f.inst.uninstall().unwrap().changed);
    }

    #[test]
    fn creates_missing_file_and_uninstall_leaves_no_hooks_key() {
        let f = fx(None);
        let r = f.inst.install(&f.rt).unwrap();
        assert!(r.changed && r.backup.is_none());
        assert_eq!(load(&f.settings)["hooks"].as_object().unwrap().len(), 6);
        f.inst.uninstall().unwrap();
        assert_eq!(load(&f.settings), json!({}));
    }

    #[test]
    fn moved_script_path_is_updated_not_duplicated() {
        let f = fx(None);
        f.inst.install(&f.rt).unwrap();
        let moved = HookInstaller::new(&f.settings, f.settings.parent().unwrap().join("other"));
        assert!(moved.install(&f.rt).unwrap().changed);
        let v = load(&f.settings);
        for kind in HookKind::ALL {
            let groups = v["hooks"][kind.name()].as_array().unwrap();
            assert_eq!(groups.len(), 1);
            assert!(
                groups[0]["hooks"][0]["command"]
                    .as_str()
                    .unwrap()
                    .contains("/other/")
            );
        }
    }

    #[test]
    fn invalid_json_is_never_touched() {
        let f = fx(Some("{ not json"));
        assert!(matches!(
            f.inst.install(&f.rt),
            Err(InstallError::InvalidJson(..))
        ));
        assert_eq!(std::fs::read_to_string(&f.settings).unwrap(), "{ not json");
    }

    #[test]
    fn unexpected_shapes_are_refused() {
        let f = fx(Some(r#"{"hooks": []}"#));
        assert!(matches!(
            f.inst.install(&f.rt),
            Err(InstallError::UnexpectedShape(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn script_is_executable_and_fails_silent() {
        use std::process::Command;
        let f = fx(None);
        f.inst.install(&f.rt).unwrap();
        // Assistant not running: no runtime files => exit 0 immediately.
        let out = Command::new(f.inst.script_path()).output().unwrap();
        assert!(out.status.success());
        // Stale port nobody listens on: still exit 0.
        f.rt.write(1, "s").unwrap();
        let out = Command::new(f.inst.script_path()).output().unwrap();
        assert!(out.status.success());
    }
}
