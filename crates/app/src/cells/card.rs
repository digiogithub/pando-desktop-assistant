//! The side card of a selected cell (PDA-T-0058): a pure model built from the cell, its task
//! and its pending permissions, including which actions the engine and status allow.

use organism::{CellId, CellInput, Engine, Status};
use pando_client::api::TaskResponse;
use pando_client::chat::PermissionRequest;

use super::source::PRIMARY_CELL_ID;

/// Lines of output shown on the card.
pub const TAIL_LINES: usize = 3;

/// What the user can do with a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CellAction {
    /// Stop a running task.
    Cancel,
    /// Remove a finished task from Pando.
    Delete,
    /// Create the same task again.
    Relaunch,
    /// Approve the pending permission.
    Approve,
    /// Deny the pending permission.
    Deny,
    /// Send what the user says next to this agent ("Hablarle").
    Talk,
}

/// What kind of thing a cell is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    /// A mesnada task.
    Task,
    /// A non-primary Pando instance.
    Instance,
    /// The primary (mesnada) session standing for the body.
    Primary,
}

/// Kind of a cell from its id.
#[must_use]
pub fn kind_of(id: &CellId) -> CellKind {
    if id.as_str() == PRIMARY_CELL_ID {
        CellKind::Primary
    } else if id.as_str().starts_with("instance:") {
        CellKind::Instance
    } else {
        CellKind::Task
    }
}

/// A pending permission as the card shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingView {
    /// Request id.
    pub id: String,
    /// Tool asking.
    pub tool: String,
    /// Description.
    pub description: String,
    /// Flagged as "explicit approval only".
    pub flagged: bool,
}

/// The structured conclusion of a finished task.
#[derive(Clone, Debug, PartialEq)]
pub struct ConclusionView {
    /// `success|partial|failed|blocked`.
    pub status: String,
    /// Confidence in percent.
    pub confidence: u8,
    /// One-paragraph summary.
    pub summary: String,
}

/// Everything the side card renders.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    /// Cell id.
    pub id: CellId,
    /// Display label.
    pub label: String,
    /// Engine (for the localized name via `organism.engine.<engine_id>`).
    pub engine: Engine,
    /// Token id of the engine.
    pub engine_id: &'static str,
    /// Status (`organism.status.<status_id>`).
    pub status: Status,
    /// Token id of the status.
    pub status_id: &'static str,
    /// Project name or folder, when the server reports one (G2).
    pub project: Option<String>,
    /// Last lines of output.
    pub tail: Vec<String>,
    /// Progress in percent (running tasks that report it).
    pub progress: Option<u8>,
    /// Tool in use.
    pub current_tool: Option<String>,
    /// Allowed actions, in display order.
    pub actions: Vec<CellAction>,
    /// "Hablarle" can be used now (the task's session is known); when it is listed but not
    /// enabled the button is disabled with a tooltip (until gap G2 exposes the session).
    pub talk_enabled: bool,
    /// Structured conclusion, when the task has one.
    pub conclusion: Option<ConclusionView>,
    /// The whole output, for the "view full output" panel.
    pub full_output: String,
    /// Permissions waiting for an answer.
    pub pending: Vec<PendingView>,
}

fn is_terminal(s: Status) -> bool {
    matches!(s, Status::Done | Status::Error)
}

/// Last `n` non-empty lines of `text`.
#[must_use]
pub fn tail_lines(text: &str, n: usize) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect();
    let skip = lines.len().saturating_sub(n);
    lines.drain(..skip);
    lines
}

fn project_of(t: &TaskResponse) -> Option<String> {
    let name = t.project_name.as_deref().filter(|n| !n.is_empty());
    let folder = || {
        t.work_dir
            .as_deref()
            .or(t.project_path.as_deref())
            .and_then(|p| std::path::Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned)
    };
    name.map(str::to_owned).or_else(folder)
}

impl Card {
    /// Builds the card. `pending` are the permissions of the cell's session.
    #[must_use]
    pub fn build(
        cell: &CellInput,
        task: Option<&TaskResponse>,
        pending: &[PermissionRequest],
    ) -> Self {
        let kind = kind_of(&cell.id);
        let pending: Vec<PendingView> = pending
            .iter()
            .map(|p| PendingView {
                id: p.id.clone(),
                tool: p.tool_name.clone(),
                description: p.description.clone(),
                flagged: p.require_explicit_approval || p.never_auto_approve,
            })
            .collect();
        let mut actions = Vec::new();
        match kind {
            CellKind::Task => {
                if is_terminal(cell.status) {
                    actions.push(CellAction::Delete);
                    if task.is_some_and(|t| !t.prompt.trim().is_empty()) {
                        actions.push(CellAction::Relaunch);
                    }
                } else {
                    actions.push(CellAction::Cancel);
                    if cell.engine.token().can_talk {
                        actions.push(CellAction::Talk);
                    }
                }
            }
            CellKind::Instance | CellKind::Primary => {}
        }
        if !pending.is_empty() && !is_terminal(cell.status) {
            actions.push(CellAction::Approve);
            actions.push(CellAction::Deny);
        }
        let talk_enabled = task
            .and_then(|t| t.acp_session_id.as_deref())
            .is_some_and(|s| !s.is_empty());
        let tail = task.map_or_else(Vec::new, |t| {
            let text = t
                .output_tail
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&t.output);
            tail_lines(text, TAIL_LINES)
        });
        Self {
            id: cell.id.clone(),
            label: cell.label.to_string(),
            engine: cell.engine,
            engine_id: cell.engine.token().id,
            status: cell.status,
            status_id: cell.status.id(),
            project: task.and_then(project_of),
            tail,
            progress: task
                .filter(|t| t.status == "running" && t.progress > 0)
                .map(|t| t.progress.clamp(0, 100) as u8),
            current_tool: task
                .map(|t| t.current_tool.clone())
                .filter(|s| !s.is_empty()),
            actions,
            talk_enabled,
            conclusion: task
                .and_then(|t| t.conclusion.as_ref())
                .map(|c| ConclusionView {
                    status: c.status.clone(),
                    confidence: (c.confidence.clamp(0.0, 1.0) * 100.0).round() as u8,
                    summary: c.summary.clone(),
                }),
            full_output: task.map(|t| t.output.clone()).unwrap_or_default(),
            pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(id: &str, engine: Engine, status: Status) -> CellInput {
        CellInput::new(id, engine, status)
    }

    fn task() -> TaskResponse {
        TaskResponse {
            id: "t".into(),
            prompt: "do it".into(),
            status: "running".into(),
            progress: 40,
            current_tool: "bash".into(),
            output: "a\n\nb\nc\nd\n".into(),
            work_dir: Some("/w/proj".into()),
            acp_session_id: Some("s-acp".into()),
            ..TaskResponse::default()
        }
    }

    fn perm(id: &str, flagged: bool) -> PermissionRequest {
        PermissionRequest {
            id: id.into(),
            session_id: "s".into(),
            tool_name: "bash".into(),
            description: "rm -rf".into(),
            require_explicit_approval: flagged,
            ..PermissionRequest::default()
        }
    }

    #[test]
    fn running_acp_task_can_be_cancelled_and_talked_to() {
        let c = Card::build(
            &cell("t", Engine::ClaudeAcp, Status::Working),
            Some(&task()),
            &[],
        );
        assert_eq!(c.actions, [CellAction::Cancel, CellAction::Talk]);
        assert!(c.talk_enabled);
        // Without the task's session (gap G2) Hablarle is listed but disabled.
        let mut t = task();
        t.acp_session_id = None;
        let c2 = Card::build(
            &cell("t", Engine::ClaudeAcp, Status::Working),
            Some(&t),
            &[],
        );
        assert!(c2.actions.contains(&CellAction::Talk) && !c2.talk_enabled);
        assert_eq!(c.tail, ["b", "c", "d"]);
        assert_eq!(c.progress, Some(40));
        assert_eq!(c.project.as_deref(), Some("proj"));
        assert_eq!(c.current_tool.as_deref(), Some("bash"));
        assert_eq!((c.engine_id, c.status_id), ("acp", "working"));
    }

    #[test]
    fn cli_cannot_be_talked_to_and_finished_tasks_offer_delete_and_relaunch() {
        let c = Card::build(&cell("t", Engine::Cli, Status::Working), Some(&task()), &[]);
        assert_eq!(c.actions, [CellAction::Cancel]);
        let c = Card::build(&cell("t", Engine::Cli, Status::Done), Some(&task()), &[]);
        assert_eq!(c.actions, [CellAction::Delete, CellAction::Relaunch]);
        let c = Card::build(&cell("t", Engine::Cli, Status::Error), None, &[]);
        assert_eq!(c.actions, [CellAction::Delete], "no prompt to relaunch");
    }

    #[test]
    fn waiting_with_pending_permissions_offers_approve_and_deny() {
        let p = [perm("p1", true)];
        let c = Card::build(
            &cell("t", Engine::ClaudeAcp, Status::Waiting),
            Some(&task()),
            &p,
        );
        assert!(c.actions.contains(&CellAction::Approve) && c.actions.contains(&CellAction::Deny));
        assert!(c.pending[0].flagged);
        // The primary cell offers only the permission actions.
        let c = Card::build(
            &cell(PRIMARY_CELL_ID, Engine::Pando, Status::Waiting),
            None,
            &p,
        );
        assert_eq!(c.actions, [CellAction::Approve, CellAction::Deny]);
        // No pending, no approval buttons.
        let c = Card::build(
            &cell(PRIMARY_CELL_ID, Engine::Pando, Status::Waiting),
            None,
            &[],
        );
        assert!(c.actions.is_empty());
    }

    #[test]
    fn instances_have_no_actions_and_tail_prefers_output_tail() {
        let c = Card::build(
            &cell("instance:x", Engine::Pando, Status::Working),
            None,
            &[],
        );
        assert!(c.actions.is_empty() && c.tail.is_empty());
        let mut t = task();
        t.output_tail = Some("x\ny".into());
        let c = Card::build(&cell("t", Engine::Pando, Status::Working), Some(&t), &[]);
        assert_eq!(c.tail, ["x", "y"]);
    }

    #[test]
    fn kinds_come_from_ids() {
        assert_eq!(kind_of(&CellId::new(PRIMARY_CELL_ID)), CellKind::Primary);
        assert_eq!(kind_of(&CellId::new("instance:1")), CellKind::Instance);
        assert_eq!(kind_of(&CellId::new("abc")), CellKind::Task);
    }
}
