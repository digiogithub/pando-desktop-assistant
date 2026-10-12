//! The pure cell source (PDA-T-0055, PDA-T-0057): turns polls of `/orchestrator/tasks`,
//! `/instances` and the primary session's pending items into the list of organism cells, with
//! birth flags, the Delegating trigger and the removal of finished cells after 10 s.
//!
//! No I/O and no clock: every call carries `now` (epoch seconds), so a sequence of polls is a
//! deterministic test. The poller that feeds it lives in `super::poller`.

use std::collections::HashSet;
use std::sync::Arc;

use organism::{CellId, CellInput, Engine, Status};
use pando_client::api::{InstanceInfo, TaskChange, TaskResponse, TaskTracker};

use super::mapping::{CustomEngines, cell_for_instance, cell_for_task};
use super::time::parse_rfc3339;
use crate::organism_view::adapter::TerminalReaper;

/// Id of the synthetic cell standing for the primary (mesnada) session.
pub const PRIMARY_CELL_ID: &str = "session:primary";

/// A finished task last updated longer ago than this is not shown when first seen (a stale
/// list on startup must not paint dozens of dead cells).
pub const STALE_AFTER_SECS: f64 = 120.0;

/// One poll of Pando.
#[derive(Clone, Debug, Default)]
pub struct Poll {
    /// `GET /orchestrator/tasks`.
    pub tasks: Vec<TaskResponse>,
    /// `GET /instances`.
    pub instances: Vec<InstanceInfo>,
    /// The primary session has pending permissions or questions.
    pub primary_pending: bool,
    /// Pending permission requests (primary session and waiting ACP sessions).
    pub permissions: Vec<pando_client::chat::PermissionRequest>,
    /// Wall clock of the poll, seconds since the Unix epoch.
    pub now: f64,
}

/// What a poll changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    /// The full list of cells to show.
    pub cells: Vec<CellInput>,
    /// A task was created since the previous poll: start the Delegating overlay.
    pub delegate: bool,
    /// Ids that appeared in this poll.
    pub born: Vec<CellId>,
    /// Ids that left the list in this poll (deleted, or finished and expired).
    pub gone: Vec<CellId>,
    /// The cell list differs from the previous one.
    pub changed: bool,
}

/// Turns polls into cells.
#[derive(Debug)]
pub struct CellSource {
    custom: CustomEngines,
    tracker: TaskTracker,
    initialised: bool,
    reaper: TerminalReaper,
    /// Finished cells already removed; never shown again unless the task is revived.
    dismissed: HashSet<String>,
    primary_label: Arc<str>,
    last: Vec<CellInput>,
    external: Vec<CellInput>,
    tasks: std::collections::HashMap<String, TaskResponse>,
}

impl CellSource {
    /// A source with no history; `primary_label` names the primary session cell.
    #[must_use]
    pub fn new(custom: CustomEngines, primary_label: &str) -> Self {
        Self {
            custom,
            tracker: TaskTracker::new(),
            initialised: false,
            reaper: TerminalReaper::new(),
            dismissed: HashSet::new(),
            primary_label: Arc::from(primary_label),
            last: Vec::new(),
            external: Vec::new(),
            tasks: std::collections::HashMap::new(),
        }
    }

    /// Replaces the custom engine table (templates changed).
    pub fn set_custom_engines(&mut self, custom: CustomEngines) {
        self.custom = custom;
    }

    /// The task behind a cell id, from the last poll.
    #[must_use]
    pub fn task(&self, id: &str) -> Option<&TaskResponse> {
        self.tasks.get(id)
    }

    /// The cells of the last update.
    #[must_use]
    pub fn cells(&self) -> &[CellInput] {
        &self.last
    }

    /// Forgets everything (connection lost / switched to another Pando): the next poll is a
    /// first poll again.
    pub fn reset(&mut self) {
        self.tracker = TaskTracker::new();
        self.initialised = false;
        self.reaper = TerminalReaper::new();
        self.dismissed.clear();
        self.last.clear();
        self.last.extend(self.external.iter().cloned());
        self.tasks.clear();
    }

    /// Replaces the external (Claude Code hook) cells. Returns whether the shown list changed.
    pub fn set_external(&mut self, external: Vec<CellInput>) -> bool {
        let old: HashSet<CellId> = self.external.iter().map(|c| c.id.clone()).collect();
        self.last.retain(|c| !old.contains(&c.id));
        self.last.extend(external.iter().cloned());
        let changed = cells_differ(&self.external, &external);
        self.external = external;
        changed
    }

    /// Applies one poll.
    pub fn update(&mut self, poll: &Poll) -> Update {
        let changes = self.tracker.update(&poll.tasks);
        self.tasks = poll
            .tasks
            .iter()
            .map(|t| (t.id.clone(), t.clone()))
            .collect();
        let first = !self.initialised;
        self.initialised = true;

        let mut born: Vec<CellId> = Vec::new();
        let mut gone: Vec<CellId> = Vec::new();
        let mut delegate = false;
        for ch in &changes {
            match ch {
                TaskChange::Appeared(t) => {
                    let stale = t.is_terminal()
                        && parse_rfc3339(&t.updated_at)
                            .is_some_and(|u| poll.now - u > STALE_AFTER_SECS);
                    if t.is_terminal() && (first || stale) {
                        self.dismissed.insert(t.id.clone());
                        continue;
                    }
                    if !first && !t.is_terminal() {
                        born.push(CellId::new(&t.id));
                        delegate = true;
                    }
                }
                TaskChange::Updated(t) | TaskChange::Finished(t) => {
                    // A revived task (relaunch) comes back.
                    if !t.is_terminal() {
                        self.dismissed.remove(&t.id);
                    }
                }
                TaskChange::Removed(id) => {
                    self.dismissed.remove(id);
                    gone.push(CellId::new(id));
                }
            }
        }

        let mut ordered: Vec<&TaskResponse> = poll
            .tasks
            .iter()
            .filter(|t| !self.dismissed.contains(&t.id))
            .collect();
        ordered.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        let mut cells: Vec<CellInput> = Vec::with_capacity(ordered.len() + 1);
        for t in ordered {
            let mut c = cell_for_task(t, &self.custom);
            c.is_new = born.iter().any(|b| b.as_str() == t.id);
            cells.push(c);
        }
        for i in poll.instances.iter().filter(|i| !i.is_primary) {
            cells.push(cell_for_instance(i));
        }
        if poll.primary_pending {
            let mut c = CellInput::new(PRIMARY_CELL_ID, Engine::Pando, Status::Waiting);
            c.label = self.primary_label.clone();
            cells.push(c);
        }

        let before: HashSet<CellId> = cells.iter().map(|c| c.id.clone()).collect();
        let expired = self.reaper.reap(poll.now, &mut cells);
        for id in expired {
            self.dismissed.insert(id.as_str().to_owned());
            gone.push(id);
        }
        // External cells are not tasks: the reaper never touches them (their tracker owns the lifetime).
        cells.extend(self.external.iter().cloned());
        let before: HashSet<CellId> = before
            .into_iter()
            .chain(self.external.iter().map(|c| c.id.clone()))
            .collect();
        // Cells that were there last time but are not any more (task deleted, instance gone).
        for old in &self.last {
            if !before.contains(&old.id) && !gone.contains(&old.id) {
                gone.push(old.id.clone());
            }
        }
        let changed = cells_differ(&self.last, &cells);
        self.last.clone_from(&cells);
        Update {
            cells,
            delegate,
            born,
            gone,
            changed,
        }
    }
}

/// Ignores `is_new` (a one-shot flag).
fn cells_differ(a: &[CellInput], b: &[CellInput]) -> bool {
    a.len() != b.len()
        || a.iter().zip(b).any(|(x, y)| {
            x.id != y.id
                || x.engine != y.engine
                || x.status != y.status
                || x.label != y.label
                || x.dim != y.dim
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: &str, created: &str) -> TaskResponse {
        TaskResponse {
            id: id.into(),
            name: format!("task {id}"),
            agent: "pando".into(),
            status: status.into(),
            created_at: created.into(),
            updated_at: "2026-10-12T07:00:00Z".into(),
            ..TaskResponse::default()
        }
    }

    fn poll(tasks: Vec<TaskResponse>, now: f64) -> Poll {
        Poll {
            tasks,
            now,
            ..Poll::default()
        }
    }

    // 2026-10-12T07:00:00Z
    const T0: f64 = 1_791_788_400.0;

    fn source() -> CellSource {
        CellSource::new(CustomEngines::none(), "mesnada")
    }

    #[test]
    fn first_poll_populates_without_birth_or_delegating() {
        let mut s = source();
        let u = s.update(&poll(
            vec![task("a", "running", "1"), task("b", "pending", "2")],
            T0,
        ));
        assert_eq!(u.cells.len(), 2);
        assert!(!u.delegate && u.born.is_empty() && u.changed);
        assert!(u.cells.iter().all(|c| !c.is_new));
        assert_eq!(u.cells[1].status, Status::Pending);
    }

    #[test]
    fn a_created_task_is_born_and_delegates_once() {
        let mut s = source();
        s.update(&poll(vec![task("a", "running", "1")], T0));
        let u = s.update(&poll(
            vec![task("a", "running", "1"), task("b", "running", "2")],
            T0 + 1.0,
        ));
        assert!(u.delegate);
        assert_eq!(u.born, vec![CellId::new("b")]);
        assert!(
            u.cells
                .iter()
                .find(|c| c.id.as_str() == "b")
                .unwrap()
                .is_new
        );
        // Next poll: nothing new, no delegating, is_new cleared.
        let u = s.update(&poll(
            vec![task("a", "running", "1"), task("b", "running", "2")],
            T0 + 2.0,
        ));
        assert!(!u.delegate && !u.changed);
        assert!(u.cells.iter().all(|c| !c.is_new));
    }

    #[test]
    fn status_changes_update_the_cell_and_finished_cells_expire_after_10_s() {
        let mut s = source();
        s.update(&poll(vec![task("a", "running", "1")], T0));
        let u = s.update(&poll(vec![task("a", "completed", "1")], T0 + 1.0));
        assert_eq!(u.cells[0].status, Status::Done);
        assert!(u.changed && !u.delegate);
        let u = s.update(&poll(vec![task("a", "completed", "1")], T0 + 10.5));
        assert_eq!(u.cells.len(), 1, "9.5 s after finishing it still shows");
        let u = s.update(&poll(vec![task("a", "completed", "1")], T0 + 11.5));
        assert!(u.cells.is_empty());
        assert_eq!(u.gone, vec![CellId::new("a")]);
        // The server keeps listing it: it must not come back.
        let u = s.update(&poll(vec![task("a", "completed", "1")], T0 + 30.0));
        assert!(u.cells.is_empty() && !u.changed);
    }

    #[test]
    fn stale_finished_tasks_on_startup_are_not_shown() {
        let mut s = source();
        let mut old = task("old", "completed", "1");
        old.updated_at = "2026-10-11T07:00:00Z".into();
        let u = s.update(&poll(vec![old, task("live", "running", "2")], T0));
        assert_eq!(u.cells.len(), 1);
        assert_eq!(u.cells[0].id.as_str(), "live");
    }

    #[test]
    fn deleted_tasks_leave_and_a_relaunched_one_returns() {
        let mut s = source();
        s.update(&poll(
            vec![task("a", "running", "1"), task("b", "running", "2")],
            T0,
        ));
        let u = s.update(&poll(vec![task("b", "running", "2")], T0 + 1.0));
        assert_eq!(u.gone, vec![CellId::new("a")]);
        // Finish and expire b, then revive it.
        s.update(&poll(vec![task("b", "failed", "2")], T0 + 2.0));
        let u = s.update(&poll(vec![task("b", "failed", "2")], T0 + 13.0));
        assert!(u.cells.is_empty());
        let u = s.update(&poll(vec![task("b", "running", "2")], T0 + 14.0));
        assert_eq!(u.cells.len(), 1);
        assert_eq!(u.cells[0].status, Status::Working);
    }

    #[test]
    fn instances_and_pending_primary_become_cells() {
        let mut s = source();
        let mut p = poll(vec![task("a", "running", "1")], T0);
        p.instances = vec![
            InstanceInfo {
                instance_id: "i1".into(),
                path: "/w/proj-a".into(),
                is_primary: false,
                ..InstanceInfo::default()
            },
            InstanceInfo {
                instance_id: "i0".into(),
                is_primary: true,
                ..InstanceInfo::default()
            },
        ];
        p.primary_pending = true;
        let u = s.update(&p);
        let ids: Vec<&str> = u.cells.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a", "instance:i1", PRIMARY_CELL_ID]);
        let primary = u.cells.last().unwrap();
        assert_eq!(
            (primary.status, primary.engine),
            (Status::Waiting, Engine::Pando)
        );
        assert_eq!(&*primary.label, "mesnada");
        // Pending resolved: the waiting cell goes away.
        p.primary_pending = false;
        let u = s.update(&p);
        assert_eq!(u.cells.len(), 2);
        assert_eq!(u.gone, vec![CellId::new(PRIMARY_CELL_ID)]);
    }

    #[test]
    fn reset_makes_the_next_poll_a_first_poll() {
        let mut s = source();
        s.update(&poll(vec![task("a", "running", "1")], T0));
        s.reset();
        let u = s.update(&poll(vec![task("a", "running", "1")], T0 + 1.0));
        assert!(!u.delegate && !u.cells[0].is_new);
    }
}
