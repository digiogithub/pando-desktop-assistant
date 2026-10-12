//! The "create agent" form (PDA-T-0060): validation and the `POST /orchestrator/tasks` body.

use pando_client::api::CreateTask;

/// Engines offered in the form (Pando's built-in engines; custom ones are appended by the
/// caller from the engine templates).
pub const DEFAULT_ENGINES: [&str; 6] = [
    "pando",
    "acp-claude",
    "claude",
    "gemini",
    "copilot",
    "opencode",
];

/// What the user typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewAgentForm {
    /// What the agent should do (required).
    pub prompt: String,
    /// Engine name; empty = the server default.
    pub engine: String,
    /// Working directory; empty = the server default.
    pub work_dir: String,
    /// Model; empty = the engine default.
    pub model: String,
    /// Comma-separated tags (the first one becomes the cell label).
    pub tags: String,
}

/// Why the form cannot be submitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormError {
    /// The prompt is empty.
    EmptyPrompt,
}

fn opt(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

impl NewAgentForm {
    /// The request body, or what is missing.
    pub fn to_request(&self) -> Result<CreateTask, FormError> {
        let prompt = self.prompt.trim();
        if prompt.is_empty() {
            return Err(FormError::EmptyPrompt);
        }
        Ok(CreateTask {
            prompt: prompt.to_owned(),
            work_dir: opt(&self.work_dir),
            model: opt(&self.model),
            engine: opt(&self.engine),
            tags: self
                .tags
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect(),
            dependencies: Vec::new(),
            background: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prompt_is_refused() {
        assert_eq!(
            NewAgentForm::default().to_request().unwrap_err(),
            FormError::EmptyPrompt
        );
        let f = NewAgentForm {
            prompt: "   ".into(),
            ..NewAgentForm::default()
        };
        assert_eq!(f.to_request().unwrap_err(), FormError::EmptyPrompt);
    }

    #[test]
    fn fields_are_trimmed_and_empty_ones_dropped() {
        let f = NewAgentForm {
            prompt: " fix the build ".into(),
            engine: "acp-claude".into(),
            work_dir: "  ".into(),
            model: String::new(),
            tags: "ci, build ,, ".into(),
        };
        let r = f.to_request().expect("valid");
        assert_eq!(r.prompt, "fix the build");
        assert_eq!(r.engine.as_deref(), Some("acp-claude"));
        assert_eq!((r.work_dir, r.model), (None, None));
        assert_eq!(r.tags, ["ci", "build"]);
        assert!(r.background);
    }
}
