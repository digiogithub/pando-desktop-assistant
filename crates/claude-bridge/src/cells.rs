//! Pure mapping from hook events to `ClaudeExternal` cells (PDA-T-0132, PDA-SP-0010.R1).
//!
//! No I/O and no clock: the caller passes `now_ms` (any monotonic millisecond counter). The app
//! feeds every [`HookEvent`] from the receiver into [`SessionTracker::apply`] and renders the
//! returned [`CellUpdate`]s (hollow blue cell born, status change, vesicle, death, speech).

use crate::events::{HookEvent, HookKind, MessagingEndpoint};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Cells with no hook activity for this long are considered dead (6 h).
pub const DEFAULT_INACTIVITY_MS: u64 = 6 * 60 * 60 * 1000;
/// Summaries are cut to this many characters.
pub const MAX_SUMMARY_CHARS: usize = 2000;

/// A Pando project known to the app (`/projects`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    pub id: String,
    pub path: PathBuf,
}

/// Visual state of the cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellStatus {
    /// Just born (hollow blue), nothing happened yet.
    Born,
    Working,
    /// Needs the user (permission prompt / question): badge `?`.
    Waiting,
    /// Turn finished; the summary is available.
    Done,
}

/// Snapshot of one external Claude Code session.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalCell {
    pub session_id: String,
    /// Session title, else the cwd's last component, else the id prefix.
    pub label: String,
    pub cwd: PathBuf,
    /// Pando project whose path contains `cwd` (longest match), if any.
    pub project_id: Option<String>,
    pub status: CellStatus,
    /// Last assistant message (Stop), truncated.
    pub summary: Option<String>,
    /// Notification text while `Waiting`.
    pub waiting_message: Option<String>,
    /// Where to send messages (SessionStart headers); never serialised or logged.
    pub messaging: Option<MessagingEndpoint>,
    /// "avísame al terminar": speak the next Stop summary.
    pub notify_when_done: bool,
}

/// What the UI must do after an event.
#[derive(Debug, Clone, PartialEq)]
pub enum CellUpdate {
    /// New cell: spawn a hollow blue cell under `cell.project_id`.
    Born(ExternalCell),
    /// Existing cell changed (status, label, project, summary).
    Changed(ExternalCell),
    /// Cell must disappear.
    Died { session_id: String, reason: String },
    /// "avísame" fired: queue `text` to be spoken for this cell (flag already consumed).
    Speak {
        session_id: String,
        label: String,
        text: String,
    },
}

struct Entry {
    cell: ExternalCell,
    last_ms: u64,
}

/// Tracks all live external sessions.
pub struct SessionTracker {
    projects: Vec<ProjectRef>,
    sessions: HashMap<String, Entry>,
    inactivity_ms: u64,
}

impl std::fmt::Debug for SessionTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionTracker")
            .field("sessions", &self.sessions.len())
            .field("projects", &self.projects.len())
            .finish()
    }
}

impl Default for SessionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            projects: Vec::new(),
            sessions: HashMap::new(),
            inactivity_ms: DEFAULT_INACTIVITY_MS,
        }
    }

    pub fn with_inactivity_ms(mut self, ms: u64) -> Self {
        self.inactivity_ms = ms;
        self
    }

    /// Current cells (unordered).
    pub fn cells(&self) -> impl Iterator<Item = &ExternalCell> {
        self.sessions.values().map(|e| &e.cell)
    }

    pub fn get(&self, session_id: &str) -> Option<&ExternalCell> {
        self.sessions.get(session_id).map(|e| &e.cell)
    }

    /// Find a session by label (case-insensitive, exact then prefix), for voice targets
    /// like "dile a api-worker ...".
    pub fn find_by_label(&self, label: &str) -> Option<&ExternalCell> {
        let l = label.trim().to_lowercase();
        let mut cells: Vec<&ExternalCell> = self.cells().collect();
        cells.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        cells
            .iter()
            .find(|c| c.label.to_lowercase() == l)
            .or_else(|| {
                cells
                    .iter()
                    .find(|c| c.label.to_lowercase().starts_with(&l))
            })
            .copied()
    }

    /// Replace the project list (e.g. after `/projects` refresh); returns cells whose project
    /// changed.
    pub fn set_projects(&mut self, projects: Vec<ProjectRef>) -> Vec<CellUpdate> {
        self.projects = projects;
        let mut out = Vec::new();
        for e in self.sessions.values_mut() {
            let pid = match_project(&self.projects, &e.cell.cwd);
            if pid != e.cell.project_id {
                e.cell.project_id = pid;
                out.push(CellUpdate::Changed(e.cell.clone()));
            }
        }
        out
    }

    /// Set or clear the "avísame al terminar" flag. Returns false for unknown sessions.
    pub fn set_notify(&mut self, session_id: &str, on: bool) -> bool {
        match self.sessions.get_mut(session_id) {
            Some(e) => {
                e.cell.notify_when_done = on;
                true
            }
            None => false,
        }
    }

    /// Kill cells with no activity for the inactivity period.
    pub fn expire(&mut self, now_ms: u64) -> Vec<CellUpdate> {
        let dead: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, e)| now_ms.saturating_sub(e.last_ms) >= self.inactivity_ms)
            .map(|(k, _)| k.clone())
            .collect();
        dead.into_iter()
            .map(|session_id| {
                self.sessions.remove(&session_id);
                CellUpdate::Died {
                    session_id,
                    reason: "inactive".into(),
                }
            })
            .collect()
    }

    /// Apply one hook event.
    pub fn apply(&mut self, ev: &HookEvent, now_ms: u64) -> Vec<CellUpdate> {
        if ev.kind == HookKind::SessionEnd {
            return match self.sessions.remove(&ev.session_id) {
                Some(_) => vec![CellUpdate::Died {
                    session_id: ev.session_id.clone(),
                    reason: ev.reason.clone().unwrap_or_else(|| "ended".into()),
                }],
                None => Vec::new(),
            };
        }

        let mut out = Vec::new();
        let born = !self.sessions.contains_key(&ev.session_id);
        if born {
            let cell = ExternalCell {
                session_id: ev.session_id.clone(),
                label: label_for(ev),
                cwd: ev.cwd.clone(),
                project_id: match_project(&self.projects, &ev.cwd),
                status: CellStatus::Born,
                summary: None,
                waiting_message: None,
                messaging: None,
                notify_when_done: false,
            };
            self.sessions.insert(
                ev.session_id.clone(),
                Entry {
                    cell,
                    last_ms: now_ms,
                },
            );
        }
        let Some(entry) = self.sessions.get_mut(&ev.session_id) else {
            return out;
        };
        entry.last_ms = now_ms;
        let before = entry.cell.clone();
        let cell = &mut entry.cell;

        // Refresh identity data any event may carry.
        if let Some(m) = &ev.messaging {
            cell.messaging = Some(m.clone());
        }
        if let Some(t) = &ev.session_title {
            cell.label = t.trim().to_owned();
        }
        if !ev.cwd.as_os_str().is_empty() && ev.cwd != cell.cwd {
            cell.cwd = ev.cwd.clone();
            cell.project_id = match_project(&self.projects, &cell.cwd);
        }

        let mut speak = None;
        match ev.kind {
            HookKind::SessionStart => {
                // A new/resumed/cleared session starts idle.
                cell.status = CellStatus::Born;
                cell.waiting_message = None;
                if matches!(ev.source.as_deref(), Some("startup" | "clear")) {
                    cell.summary = None;
                }
            }
            HookKind::UserPromptSubmit | HookKind::PreToolUse => {
                cell.status = CellStatus::Working;
                cell.waiting_message = None;
            }
            HookKind::Notification => match ev.notification_type.as_deref() {
                // Informational: no state change.
                Some("idle_prompt" | "auth_success") => {}
                _ => {
                    cell.status = CellStatus::Waiting;
                    cell.waiting_message = ev.message.clone().filter(|m| !m.trim().is_empty());
                }
            },
            HookKind::Stop => {
                cell.status = CellStatus::Done;
                cell.waiting_message = None;
                if let Some(msg) = ev.last_assistant_message.as_deref() {
                    let s = truncate_chars(msg.trim(), MAX_SUMMARY_CHARS);
                    if !s.is_empty() {
                        cell.summary = Some(s);
                    }
                }
                if cell.notify_when_done {
                    cell.notify_when_done = false;
                    speak = Some(CellUpdate::Speak {
                        session_id: cell.session_id.clone(),
                        label: cell.label.clone(),
                        text: cell.summary.clone().unwrap_or_else(|| cell.label.clone()),
                    });
                }
            }
            HookKind::SessionEnd => {}
        }

        if born {
            out.push(CellUpdate::Born(cell.clone()));
        } else if *cell != before {
            out.push(CellUpdate::Changed(cell.clone()));
        }
        out.extend(speak);
        out
    }
}

fn label_for(ev: &HookEvent) -> String {
    ev.session_title
        .as_deref()
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .or_else(|| ev.cwd.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| ev.session_id.chars().take(8).collect())
}

/// Longest project path that contains `cwd` (component-wise, so `/a/bc` is not under `/a/b`).
pub fn match_project(projects: &[ProjectRef], cwd: &Path) -> Option<String> {
    projects
        .iter()
        .filter(|p| !p.path.as_os_str().is_empty() && cwd.starts_with(&p.path))
        .max_by_key(|p| p.path.components().count())
        .map(|p| p.id.clone())
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: HookKind, sid: &str, cwd: &str) -> HookEvent {
        HookEvent {
            kind,
            session_id: sid.into(),
            cwd: cwd.into(),
            session_title: None,
            source: None,
            message: None,
            notification_type: None,
            last_assistant_message: None,
            prompt: None,
            tool_name: None,
            reason: None,
            agent_id: None,
            messaging: None,
        }
    }

    fn projects() -> Vec<ProjectRef> {
        vec![
            ProjectRef {
                id: "bitacora".into(),
                path: "/home/u/www/bitacora".into(),
            },
            ProjectRef {
                id: "bitacora-sub".into(),
                path: "/home/u/www/bitacora/sub".into(),
            },
            ProjectRef {
                id: "other".into(),
                path: "/home/u/www/bit".into(),
            },
        ]
    }

    #[test]
    fn session_start_births_hollow_cell_under_project() {
        let mut t = SessionTracker::new();
        t.set_projects(projects());
        let mut e = ev(
            HookKind::SessionStart,
            "s1",
            "/home/u/www/bitacora/crates/x",
        );
        e.messaging = Some(MessagingEndpoint {
            socket: "/tmp/s".into(),
            token: None,
        });
        let up = t.apply(&e, 0);
        let [CellUpdate::Born(c)] = up.as_slice() else {
            panic!("{up:?}")
        };
        assert_eq!(c.status, CellStatus::Born);
        assert_eq!(c.project_id.as_deref(), Some("bitacora"));
        assert_eq!(c.label, "x");
        assert!(c.messaging.is_some());
    }

    #[test]
    fn project_match_is_longest_and_component_wise() {
        let p = projects();
        assert_eq!(
            match_project(&p, Path::new("/home/u/www/bitacora/sub/a")).as_deref(),
            Some("bitacora-sub")
        );
        assert_eq!(match_project(&p, Path::new("/home/u/www/bitx")), None);
        assert_eq!(
            match_project(&p, Path::new("/home/u/www/bit")).as_deref(),
            Some("other")
        );
    }

    #[test]
    fn lifecycle_working_waiting_done_end() {
        let mut t = SessionTracker::new();
        t.apply(&ev(HookKind::SessionStart, "s", "/p"), 0);
        let up = t.apply(&ev(HookKind::UserPromptSubmit, "s", "/p"), 1);
        assert!(matches!(&up[..], [CellUpdate::Changed(c)] if c.status == CellStatus::Working));
        // Repeating the same state is not a change.
        assert!(t.apply(&ev(HookKind::PreToolUse, "s", "/p"), 2).is_empty());

        let mut n = ev(HookKind::Notification, "s", "/p");
        n.notification_type = Some("permission_prompt".into());
        n.message = Some("Claude needs your permission".into());
        let up = t.apply(&n, 3);
        assert!(matches!(&up[..], [CellUpdate::Changed(c)]
            if c.status == CellStatus::Waiting && c.waiting_message.is_some()));

        let mut idle = ev(HookKind::Notification, "s", "/p");
        idle.notification_type = Some("idle_prompt".into());
        assert!(t.apply(&idle, 4).is_empty());

        let mut stop = ev(HookKind::Stop, "s", "/p");
        stop.last_assistant_message = Some("  all done  ".into());
        let up = t.apply(&stop, 5);
        assert!(matches!(&up[..], [CellUpdate::Changed(c)]
            if c.status == CellStatus::Done && c.summary.as_deref() == Some("all done")));

        let mut end = ev(HookKind::SessionEnd, "s", "/p");
        end.reason = Some("logout".into());
        assert_eq!(
            t.apply(&end, 6),
            vec![CellUpdate::Died {
                session_id: "s".into(),
                reason: "logout".into()
            }]
        );
        assert!(t.cells().next().is_none());
        assert!(t.apply(&end, 7).is_empty());
    }

    #[test]
    fn avisame_speaks_once_on_stop() {
        let mut t = SessionTracker::new();
        t.apply(&ev(HookKind::SessionStart, "s", "/p"), 0);
        assert!(t.set_notify("s", true));
        assert!(!t.set_notify("nope", true));
        let mut stop = ev(HookKind::Stop, "s", "/p");
        stop.last_assistant_message = Some("Terminé la migración".into());
        let up = t.apply(&stop, 1);
        assert!(up.iter().any(
            |u| matches!(u, CellUpdate::Speak { text, .. } if text == "Terminé la migración")
        ));
        // Flag consumed.
        stop.last_assistant_message = Some("otra".into());
        assert!(
            !t.apply(&stop, 2)
                .iter()
                .any(|u| matches!(u, CellUpdate::Speak { .. }))
        );
    }

    #[test]
    fn unknown_session_is_adopted_lazily_and_title_wins() {
        let mut t = SessionTracker::new();
        let mut e = ev(HookKind::PreToolUse, "late", "/work/api");
        let up = t.apply(&e, 0);
        assert!(
            matches!(&up[..], [CellUpdate::Born(c)] if c.status == CellStatus::Working && c.label == "api")
        );
        e.session_title = Some("api-worker".into());
        let up = t.apply(&e, 1);
        assert!(matches!(&up[..], [CellUpdate::Changed(c)] if c.label == "api-worker"));
        assert_eq!(t.find_by_label("API-w").unwrap().session_id, "late");
    }

    #[test]
    fn projects_refresh_reassigns_and_inactivity_kills() {
        let mut t = SessionTracker::new().with_inactivity_ms(100);
        t.apply(&ev(HookKind::SessionStart, "s", "/home/u/www/bitacora"), 0);
        assert_eq!(t.get("s").unwrap().project_id, None);
        let up = t.set_projects(projects());
        assert!(
            matches!(&up[..], [CellUpdate::Changed(c)] if c.project_id.as_deref() == Some("bitacora"))
        );
        assert!(t.expire(50).is_empty());
        assert!(
            matches!(&t.expire(100)[..], [CellUpdate::Died { reason, .. }] if reason == "inactive")
        );
    }

    #[test]
    fn summary_is_truncated() {
        let mut t = SessionTracker::new();
        let mut stop = ev(HookKind::Stop, "s", "/p");
        stop.last_assistant_message = Some("é".repeat(MAX_SUMMARY_CHARS + 50));
        t.apply(&stop, 0);
        assert_eq!(
            t.get("s")
                .unwrap()
                .summary
                .as_ref()
                .unwrap()
                .chars()
                .count(),
            MAX_SUMMARY_CHARS + 1
        );
    }
}
