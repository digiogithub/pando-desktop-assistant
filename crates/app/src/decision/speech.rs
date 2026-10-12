//! What the assistant says after a command (never produced by an LLM).

use decide::command::{AgentStatus, CommandResult, Refusal};
use decide::policy::ConfirmReason;
use decide::policy::ControlRequest;
use decide::questions::{Action, AgentTarget};
use rust_i18n::t;

use super::directory::AgentDirectory;
use super::executor::ExecError;

fn label(dir: &AgentDirectory, slug: &str) -> String {
    dir.resolve(slug)
        .map_or_else(|| slug.to_owned(), |e| e.label.clone())
}

fn status_line(dir: &AgentDirectory, a: &AgentStatus) -> String {
    let name = label(dir, &a.id);
    let pct = a.progress.map(|p| (p * 100.0).round() as i64);
    let status = t!(
        format!("voice.cmd.status_word.{}", a.status),
        default = a.status.as_str()
    )
    .to_string();
    let mut s = match pct {
        Some(p) if a.status == "running" => {
            t!("voice.cmd.status_running", agent = name, pct = p).to_string()
        }
        _ => t!("voice.cmd.status_other", agent = name, status = status).to_string(),
    };
    if let Some(tool) = a.current_tool.as_deref().filter(|t| !t.is_empty()) {
        s.push(' ');
        s.push_str(&t!("voice.cmd.using_tool", tool = tool));
    }
    s
}

/// Speech for a finished command.
#[must_use]
pub fn command_speech(dir: &AgentDirectory, result: &Result<CommandResult, ExecError>) -> String {
    match result {
        Ok(CommandResult::Done) => t!("voice.cmd.done").to_string(),
        Ok(CommandResult::Spawned { .. }) => t!("voice.cmd.spawned").to_string(),
        Ok(CommandResult::Status { agents }) if agents.is_empty() => {
            t!("voice.cmd.no_agents").to_string()
        }
        Ok(CommandResult::Status { agents }) => agents
            .iter()
            .take(4)
            .map(|a| status_line(dir, a))
            .collect::<Vec<_>>()
            .join(" "),
        Err(ExecError::NothingPending) => t!("voice.cmd.nothing_pending").to_string(),
        Err(ExecError::NeedsExplicit) => t!("voice.cmd.needs_screen").to_string(),
        Err(ExecError::UnknownAgent(a)) => t!("voice.cmd.unknown_agent", agent = a).to_string(),
        Err(ExecError::Unsupported) => t!("voice.cmd.unavailable").to_string(),
        Err(ExecError::Api(why)) => t!("voice.cmd.failed", why = why).to_string(),
    }
}

/// Speech for a refusal by `decide::command::dispatch` (pause/resume before gap G4).
#[must_use]
pub fn refusal_speech(r: &Refusal) -> String {
    match r {
        Refusal::Unsupported(_) => t!("voice.cmd.unavailable").to_string(),
        Refusal::MissingTarget(_) => t!("voice.cmd.missing_target").to_string(),
    }
}

/// The question asked before a risky command.
#[must_use]
pub fn confirm_prompt(
    dir: &AgentDirectory,
    request: &ControlRequest,
    reason: ConfirmReason,
) -> String {
    let who = match &request.target {
        AgentTarget::Agent(slug) => label(dir, slug),
        AgentTarget::All => t!("voice.confirm.all_agents").to_string(),
        AgentTarget::None => String::new(),
    };
    let key = match request.action {
        Action::Cancel => "voice.confirm.cancel",
        Action::Deny => "voice.confirm.deny",
        Action::Retry => "voice.confirm.retry",
        Action::Spawn => "voice.confirm.spawn",
        Action::Approve => "voice.confirm.approve",
        _ => "voice.confirm.generic",
    };
    let _ = reason;
    t!(key, agent = who).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> AgentDirectory {
        AgentDirectory::from_rows([("t1".into(), "Migración auth".into(), "running".into(), None)])
    }

    fn es() {
        crate::i18n::apply(Some("es"));
    }

    #[test]
    fn status_is_a_natural_sentence() {
        es();
        let r = Ok(CommandResult::Status {
            agents: vec![AgentStatus {
                id: "migracion-auth".into(),
                status: "running".into(),
                progress: Some(0.4),
                current_tool: Some("edit".into()),
            }],
        });
        let s = command_speech(&dir(), &r);
        assert!(s.contains("Migración auth") && s.contains("40"), "{s}");
        assert!(s.contains("edit"));
    }

    #[test]
    fn errors_are_short_and_honest() {
        es();
        assert!(command_speech(&dir(), &Err(ExecError::NeedsExplicit)).contains("pantalla"));
        assert!(
            command_speech(&dir(), &Err(ExecError::Unsupported))
                .to_lowercase()
                .contains("disponible")
        );
        assert!(command_speech(&dir(), &Err(ExecError::Api("500".into()))).contains("500"));
        assert_eq!(command_speech(&dir(), &Ok(CommandResult::Done)), "Hecho.");
    }

    #[test]
    fn the_confirmation_names_the_agent() {
        es();
        let req = ControlRequest {
            action: Action::Cancel,
            target: AgentTarget::Agent("migracion-auth".into()),
            engine: None,
            project: None,
            p: 0.97,
        };
        assert_eq!(
            confirm_prompt(&dir(), &req, ConfirmReason::Destructive),
            "¿Cancelo Migración auth?"
        );
        let all = ControlRequest {
            target: AgentTarget::All,
            ..req
        };
        assert!(confirm_prompt(&dir(), &all, ConfirmReason::AllAgents).contains("todos"));
    }
}
