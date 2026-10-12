//! Pure mapping from Pando data to organism cells (PDA-T-0056, `docs/06-ui-mapping.md`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use organism::{CellInput, Engine, Status};
use pando_client::api::{InstanceInfo, TaskResponse};

/// Longest label taken from a task name.
pub const LABEL_CHARS: usize = 24;

/// How a custom engine (`.pando/mesnada/engines/*.template.yaml`) prints its output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomEngine {
    /// Engine name as used in `TaskResponse.agent`.
    pub name: String,
    /// `output_format` of the template (`text`, `json`, `stream-json`, `acp`, ...).
    pub output_format: String,
}

/// Custom engines by name, loaded from the engine templates.
#[derive(Clone, Debug, Default)]
pub struct CustomEngines(HashMap<String, CustomEngine>);

impl CustomEngines {
    /// No custom engines.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Registers one engine.
    pub fn insert(&mut self, engine: CustomEngine) {
        self.0.insert(engine.name.clone(), engine);
    }

    /// Reads every `*.template.yaml` of `dir` (`name:` and `output_format:` top-level keys; a
    /// missing `name` falls back to the file stem). Unreadable files are skipped.
    #[must_use]
    pub fn load_dir(dir: &Path) -> Self {
        let mut out = Self::none();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            let Some(stem) = file.strip_suffix(".template.yaml") else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(engine) = parse_template(stem, &text) {
                out.insert(engine);
            }
        }
        out
    }

    fn get(&self, name: &str) -> Option<&CustomEngine> {
        self.0.get(name)
    }
}

fn parse_template(stem: &str, text: &str) -> Option<CustomEngine> {
    let mut name = None;
    let mut format = None;
    for line in text.lines() {
        // Top-level keys only.
        if line.starts_with(' ') || line.starts_with('\t') || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim().trim_matches(['"', '\'']).to_owned();
        match k.trim() {
            "name" if !v.is_empty() => name = Some(v),
            "output_format" if !v.is_empty() => format = Some(v),
            _ => {}
        }
    }
    Some(CustomEngine {
        name: name.unwrap_or_else(|| stem.to_owned()),
        output_format: format?,
    })
}

/// Engine of a task from `TaskResponse.agent`.
#[must_use]
pub fn engine_for_agent(agent: &str, custom: &CustomEngines) -> Engine {
    let a = agent.trim().to_ascii_lowercase();
    if let Some(c) = custom.get(&a).or_else(|| custom.get(agent)) {
        return if c.output_format.eq_ignore_ascii_case("acp") {
            Engine::ClaudeAcp
        } else {
            Engine::Cli
        };
    }
    match a.as_str() {
        "pando" => Engine::Pando,
        "warm-acp" | "acp" => Engine::ClaudeAcp,
        s if s.starts_with("acp-") => Engine::ClaudeAcp,
        // claude, copilot, gemini, opencode, mistral, ollama-*, unknown text engines.
        _ => Engine::Cli,
    }
}

/// Status of a task from `TaskResponse.status`; `cancelled` ends as `Done` (the cell then
/// dies after 10 s like any finished one).
#[must_use]
pub fn status_for_task(status: &str) -> Status {
    match status {
        "pending" => Status::Pending,
        "paused" => Status::Paused,
        "completed" | "cancelled" => Status::Done,
        "failed" => Status::Error,
        // `running` and anything the server may add later.
        _ => Status::Working,
    }
}

/// Whether the ACP session of a running task waits for a permission (gap G2).
#[must_use]
pub fn waits_for_permission(t: &TaskResponse) -> bool {
    t.acp_status
        .as_ref()
        .is_some_and(|a| a.mode.eq_ignore_ascii_case("permission"))
        || t.acp_mode
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case("permission"))
}

/// Fits `s` into [`LABEL_CHARS`] characters: longer text is cut at a word boundary when one exists
/// past the middle, and ends in an ellipsis so a clipped label never looks like a complete one.
#[must_use]
pub fn fit_label(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= LABEL_CHARS {
        return s.to_owned();
    }
    let head: String = s.chars().take(LABEL_CHARS - 1).collect();
    let word_complete = s
        .chars()
        .nth(LABEL_CHARS - 1)
        .is_some_and(char::is_whitespace);
    let cut = if word_complete {
        head.as_str()
    } else {
        head.rfind(' ')
            .filter(|&i| head[..i].chars().count() >= LABEL_CHARS / 2)
            .map_or(head.as_str(), |i| &head[..i])
    };
    format!("{}…", cut.trim_end())
}

/// Label of a task: the first tag, else the name (or prompt) fitted to [`LABEL_CHARS`].
#[must_use]
pub fn label_for_task(t: &TaskResponse) -> String {
    let tag = t
        .tags
        .as_ref()
        .and_then(|tags| tags.iter().find(|s| !s.trim().is_empty()));
    if let Some(tag) = tag {
        return fit_label(tag);
    }
    let name = t.name.trim();
    let src = if name.is_empty() || name.starts_with(PREAMBLE) {
        // Pando truncates `name` inside the preamble: the prompt has the real text.
        strip_task_preamble(&t.prompt)
    } else {
        name
    };
    fit_label(src)
}

const PREAMBLE: &str = "You are the task_id:";

/// Pando prefixes delegated prompts with `You are the task_id: task-1234\n\n` and suffixes a
/// `---` conclusion instruction: the label is the real prompt between them.
fn strip_task_preamble(src: &str) -> &str {
    let mut src = src.trim();
    if src.starts_with(PREAMBLE) {
        src = src.split_once("\n\n").map_or(src, |(_, rest)| rest.trim());
    }
    src.split_once("\n\n---")
        .map_or(src, |(head, _)| head.trim())
}

/// The cell for a task (`is_new` is the diff's business, not the mapping's).
#[must_use]
pub fn cell_for_task(t: &TaskResponse, custom: &CustomEngines) -> CellInput {
    let mut status = status_for_task(&t.status);
    if status == Status::Working && waits_for_permission(t) {
        status = Status::Waiting;
    }
    let mut c = CellInput::new(&t.id, engine_for_agent(&t.agent, custom), status);
    c.label = Arc::from(label_for_task(t));
    c
}

/// A non-primary Pando instance as a cell (`/instances`, warm per-project instances).
#[must_use]
pub fn cell_for_instance(i: &InstanceInfo) -> CellInput {
    let id = format!("instance:{}", i.instance_id);
    let mut c = CellInput::new(&id, Engine::Pando, Status::Working);
    let name = Path::new(&i.path)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty() && *n != ".")
        .unwrap_or(&i.instance_id);
    c.label = Arc::from(fit_label(name));
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use pando_client::api::AcpStatus;

    fn task(agent: &str, status: &str) -> TaskResponse {
        TaskResponse {
            id: "t1".into(),
            name: "Refactor the GPUI shell for the new organism view".into(),
            agent: agent.into(),
            status: status.into(),
            ..TaskResponse::default()
        }
    }

    #[test]
    fn full_engine_by_status_table() {
        let custom = CustomEngines::none();
        let engines = [
            ("pando", Engine::Pando),
            ("acp-claude", Engine::ClaudeAcp),
            ("warm-acp", Engine::ClaudeAcp),
            ("acp-gemini", Engine::ClaudeAcp),
            ("claude", Engine::Cli),
            ("copilot", Engine::Cli),
            ("gemini", Engine::Cli),
            ("opencode", Engine::Cli),
            ("mistral", Engine::Cli),
            ("ollama-llama3", Engine::Cli),
            ("something-new", Engine::Cli),
        ];
        let statuses = [
            ("pending", Status::Pending),
            ("running", Status::Working),
            ("paused", Status::Paused),
            ("completed", Status::Done),
            ("failed", Status::Error),
            ("cancelled", Status::Done),
        ];
        for (agent, engine) in engines {
            for (raw, status) in statuses {
                let c = cell_for_task(&task(agent, raw), &custom);
                assert_eq!((c.engine, c.status), (engine, status), "{agent}/{raw}");
                assert_eq!(c.id.as_str(), "t1");
            }
        }
    }

    #[test]
    fn label_is_the_first_tag_or_24_chars() {
        let mut t = task("pando", "running");
        assert!(label_for_task(&t).chars().count() <= LABEL_CHARS);
        assert_eq!(label_for_task(&t), "Refactor the GPUI shell…");
        t.tags = Some(vec![String::new(), " migracion-auth ".into()]);
        assert_eq!(label_for_task(&t), "migracion-auth");
        t.tags = None;
        t.name.clear();
        t.prompt = "añade pruebas al módulo de pagos y despliega".into();
        assert_eq!(label_for_task(&t), "añade pruebas al módulo…");
    }

    #[test]
    fn fit_label_ellipsizes_long_ids_and_keeps_short_text() {
        assert_eq!(fit_label("  short  "), "short");
        let id = "46aca4c4-d3d4-4490-92fd-aaaabbbbcccc";
        let f = fit_label(id);
        assert_eq!(f.chars().count(), LABEL_CHARS);
        assert!(f.ends_with('…'));
        assert_eq!(
            fit_label("Run the bash command: sl"),
            "Run the bash command: sl"
        );
    }

    #[test]
    fn label_skips_the_delegation_preamble() {
        let mut t = task("pando", "running");
        t.tags = None;
        t.name = "You are the task_id: tas".into();
        t.prompt = "You are the task_id: task-06767be4\n\nReply with exactly the word: pong\n\n---\nIMPORTANT".into();
        assert_eq!(label_for_task(&t), "Reply with exactly the…");
    }

    #[test]
    fn permission_mode_makes_a_running_acp_task_wait() {
        let mut t = task("acp-claude", "running");
        t.acp_status = Some(AcpStatus {
            mode: "permission".into(),
            ..AcpStatus::default()
        });
        assert_eq!(
            cell_for_task(&t, &CustomEngines::none()).status,
            Status::Waiting
        );
        t.status = "completed".into();
        assert_eq!(
            cell_for_task(&t, &CustomEngines::none()).status,
            Status::Done
        );
    }

    #[test]
    fn custom_engines_follow_their_output_format() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("zed.template.yaml"),
            "name: zed\noutput_format: acp\ncommand: zed\n",
        )
        .expect("write");
        std::fs::write(
            dir.path().join("aider.template.yaml"),
            "# comment\noutput_format: \"text\"\nargs:\n  - name: ignored\n",
        )
        .expect("write");
        std::fs::write(dir.path().join("notes.txt"), "output_format: acp").expect("write");
        let custom = CustomEngines::load_dir(dir.path());
        assert_eq!(engine_for_agent("zed", &custom), Engine::ClaudeAcp);
        assert_eq!(engine_for_agent("aider", &custom), Engine::Cli);
        assert_eq!(engine_for_agent("notes", &custom), Engine::Cli);
        assert!(
            CustomEngines::load_dir(Path::new("/nonexistent"))
                .0
                .is_empty()
        );
    }

    #[test]
    fn instances_become_pando_cells_labelled_by_folder() {
        let i = InstanceInfo {
            instance_id: "abc".into(),
            path: "/home/u/work/my-project".into(),
            ..InstanceInfo::default()
        };
        let c = cell_for_instance(&i);
        assert_eq!(c.id.as_str(), "instance:abc");
        assert_eq!(c.engine, Engine::Pando);
        assert_eq!(&*c.label, "my-project");
    }
}
