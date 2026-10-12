//! Show / hide shortcuts (PDA-T-0119): the `ToggleMini` and `ShowMain` actions, their default
//! key bindings and the user keymap (`keymap.json` in the config directory).
//!
//! ```json
//! [ { "context": null, "bindings": { "ctrl-alt-m": "pando::ToggleMini", "ctrl-alt-p": "pando::ShowMain" } } ]
//! ```
//!
//! The actions are also reachable from the application menu and the tray.

use std::path::Path;

use serde::Deserialize;

use super::controller::Msg;
use super::ipc::Command;
use crate::ui::{App, KeyBinding};

crate::ui::actions!(pando, [ToggleMini, ShowMain]);

/// Default bindings (in-app, not global).
pub const DEFAULTS: &[(&str, &str)] = &[
    ("ctrl-shift-m", "pando::ToggleMini"),
    ("ctrl-shift-p", "pando::ShowMain"),
];

/// One block of the keymap file.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Block {
    /// Key context predicate, `null` for everywhere.
    #[serde(default)]
    pub context: Option<String>,
    /// Keystroke to action name.
    #[serde(default)]
    pub bindings: std::collections::BTreeMap<String, String>,
}

/// Parses the keymap JSON; only the show / hide actions of this module are accepted.
pub fn parse(json: &str) -> Result<Vec<(Option<String>, String, String)>, String> {
    let blocks: Vec<Block> = serde_json::from_str(json).map_err(|e| format!("keymap.json: {e}"))?;
    let mut out = Vec::new();
    for b in blocks {
        for (keys, action) in b.bindings {
            if !matches!(action.as_str(), "pando::ToggleMini" | "pando::ShowMain") {
                return Err(format!("keymap.json: unknown action {action:?}"));
            }
            out.push((b.context.clone(), keys, action));
        }
    }
    Ok(out)
}

fn binding(cx: &App, keys: &str, action: &str, context: Option<&str>) -> Option<KeyBinding> {
    let act = crate::ui::paint::build_action(cx, action).ok()?;
    crate::ui::key_binding(keys, act, context).ok()
}

/// Registers the global action handlers and the key bindings (defaults first, then the user's
/// file, which therefore wins). A broken user file is logged and ignored.
pub fn install(cx: &mut App, inbox: async_channel::Sender<Msg>, user_keymap: Option<&Path>) {
    let tx = inbox.clone();
    cx.on_action(move |_: &ToggleMini, _| {
        let _ = tx.try_send(Msg::Command(Command::ToggleMini));
    });
    cx.on_action(move |_: &ShowMain, _| {
        let _ = inbox.try_send(Msg::Command(Command::Show));
    });
    let mut bindings: Vec<KeyBinding> = DEFAULTS
        .iter()
        .filter_map(|(k, a)| binding(cx, k, a, None))
        .collect();
    if let Some(path) = user_keymap
        && let Ok(text) = std::fs::read_to_string(path)
    {
        match parse(&text) {
            Ok(entries) => bindings.extend(
                entries
                    .iter()
                    .filter_map(|(ctx, k, a)| binding(cx, k, a, ctx.as_deref())),
            ),
            Err(e) => tracing::warn!("{e}"),
        }
    }
    cx.bind_keys(bindings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_keymap_parses_and_rejects_unknown_actions() {
        let ok = parse(
            r#"[{"context":null,"bindings":{"ctrl-alt-m":"pando::ToggleMini"}},
                {"context":"Workspace","bindings":{"f9":"pando::ShowMain"}}]"#,
        )
        .expect("parse");
        assert_eq!(ok.len(), 2);
        assert_eq!(
            ok[1],
            (
                Some("Workspace".into()),
                "f9".into(),
                "pando::ShowMain".into()
            )
        );
        assert!(parse(r#"[{"bindings":{"a":"pando::Nope"}}]"#).is_err());
        assert!(parse("not json").is_err());
        assert!(parse("[]").expect("empty").is_empty());
    }
}
