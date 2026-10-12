//! Toasts and system notifications (PDA-T-0128, PDA-SP-0009.R7).
//!
//! * Toasts come from `/notifications/stream` ([`toast_for`], [`ToastService`]).
//! * OS notifications fire only while the app is unfocused, for: an agent waiting for the user,
//!   an agent finished (only when the user asked to be told), and a lost connection
//!   ([`decide`], [`CellWatcher`]). Clicking one focuses the cell (`Command::FocusCell`).
//!
//! The delivery commands (`notify-send`, `osascript`, PowerShell) are built by pure functions;
//! click-to-focus is implemented for Linux (`notify-send --action --wait`, libnotify >= 0.8);
//! macOS and Windows show the notification but cannot call back yet (pending, see the task).

use std::collections::{HashMap, HashSet};
use std::process::{Command as Process, Stdio};

use async_channel::Sender;
use organism::{CellId, CellInput, Status};
use pando_client::connection::Snapshot;
use pando_client::events::{Notification, NotificationEvent};
use pando_client::stream::{EventStreamConfig, StreamItem};
use rust_i18n::t;
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::controller::Msg;
use super::ipc::Command;

// ------------------------------------------------------------------------------ toasts

/// Severity of a toast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastLevel {
    /// Informational.
    Info,
    /// Needs attention.
    Warning,
    /// Failure.
    Error,
}

/// A toast ready to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastSpec {
    /// Severity.
    pub level: ToastLevel,
    /// Text (already prefixed with its source for non-system toasts).
    pub message: String,
    /// Seconds on screen; `None` = sticky (`ttl` 0).
    pub seconds: Option<u64>,
}

/// Maps a stream notification to a toast; empty messages are dropped.
#[must_use]
pub fn toast_for(n: &Notification) -> Option<ToastSpec> {
    let message = n.message.trim();
    if message.is_empty() {
        return None;
    }
    let level = match n.level.as_str() {
        "error" => ToastLevel::Error,
        "warn" | "warning" => ToastLevel::Warning,
        _ => ToastLevel::Info,
    };
    let message = match n.source.as_str() {
        "" | "system" => message.to_owned(),
        src => format!("{src}: {message}"),
    };
    let seconds = (n.ttl > 0).then(|| (n.ttl / 1_000_000_000).max(1) as u64);
    Some(ToastSpec {
        level,
        message,
        seconds,
    })
}

/// Follows the connection and forwards toasts; dropping it stops the stream.
#[derive(Debug)]
pub struct ToastService {
    task: JoinHandle<()>,
}

impl ToastService {
    /// Starts streaming `/notifications/stream` while a client is available.
    pub fn start(
        handle: &Handle,
        mut snapshot: watch::Receiver<Snapshot>,
        tx: Sender<ToastSpec>,
    ) -> Self {
        let task = handle.spawn(async move {
            loop {
                let client = snapshot.borrow_and_update().client.clone();
                let Some(client) = client else {
                    if snapshot.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                let mut stream = client.notifications(EventStreamConfig::default());
                loop {
                    tokio::select! {
                        item = stream.next() => match item {
                            Some(StreamItem::Event(NotificationEvent::Notification(n))) => {
                                if let Some(t) = toast_for(&n)
                                    && tx.send(t).await.is_err()
                                {
                                    return;
                                }
                            }
                            Some(_) => {}
                            None => return,
                        },
                        changed = snapshot.changed() => {
                            if changed.is_err() {
                                return;
                            }
                            break; // new generation or disconnected: reopen
                        }
                    }
                }
            }
        });
        Self { task }
    }
}

impl Drop for ToastService {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// ----------------------------------------------------------------------- OS notifications

/// What happened that might deserve an OS notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotifyEvent {
    /// An agent needs the user (a permission, a question).
    AgentWaiting {
        /// Cell id.
        cell: String,
        /// Label.
        label: String,
    },
    /// An agent finished.
    AgentFinished {
        /// Cell id.
        cell: String,
        /// Label.
        label: String,
        /// It ended in error.
        failed: bool,
    },
    /// The connection to Pando dropped.
    ConnectionLost,
}

/// An OS notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsNotice {
    /// Title.
    pub title: String,
    /// Body.
    pub body: String,
    /// Cell to focus when clicked.
    pub cell: Option<String>,
}

/// Decides whether `event` becomes an OS notification: never while the app is focused or when
/// the user disabled them; "finished" only for agents the user asked to be told about.
#[must_use]
pub fn decide(
    event: &NotifyEvent,
    enabled: bool,
    app_focused: bool,
    notify_when_done: &HashSet<String>,
) -> Option<OsNotice> {
    if !enabled || app_focused {
        return None;
    }
    match event {
        NotifyEvent::AgentWaiting { cell, label } => Some(OsNotice {
            title: t!("shell.notify.waiting_title").to_string(),
            body: t!("shell.notify.waiting_body", label = label).to_string(),
            cell: Some(cell.clone()),
        }),
        NotifyEvent::AgentFinished {
            cell,
            label,
            failed,
        } => notify_when_done.contains(cell).then(|| OsNotice {
            title: t!(if *failed {
                "shell.notify.failed_title"
            } else {
                "shell.notify.finished_title"
            })
            .to_string(),
            body: t!("shell.notify.finished_body", label = label).to_string(),
            cell: Some(cell.clone()),
        }),
        NotifyEvent::ConnectionLost => Some(OsNotice {
            title: t!("shell.notify.lost_title").to_string(),
            body: t!("shell.notify.lost_body").to_string(),
            cell: None,
        }),
    }
}

/// Turns successive cell lists into events (the first list only seeds the state).
#[derive(Clone, Debug, Default)]
pub struct CellWatcher {
    seen: Option<HashMap<CellId, Status>>,
}

impl CellWatcher {
    /// A watcher that has seen nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget everything (reconnect).
    pub fn reset(&mut self) {
        self.seen = None;
    }

    /// Events caused by the transition to `cells`.
    pub fn observe(&mut self, cells: &[CellInput]) -> Vec<NotifyEvent> {
        let now: HashMap<CellId, Status> = cells.iter().map(|c| (c.id.clone(), c.status)).collect();
        let mut out = Vec::new();
        if let Some(prev) = &self.seen {
            for c in cells {
                let before = prev.get(&c.id).copied();
                let id = c.id.as_str().to_owned();
                let label = c.label.to_string();
                match (before, c.status) {
                    (b, Status::Waiting) if b != Some(Status::Waiting) => {
                        out.push(NotifyEvent::AgentWaiting { cell: id, label });
                    }
                    (Some(b), Status::Done | Status::Error)
                        if !matches!(b, Status::Done | Status::Error) =>
                    {
                        out.push(NotifyEvent::AgentFinished {
                            cell: id,
                            label,
                            failed: c.status == Status::Error,
                        });
                    }
                    _ => {}
                }
            }
        }
        self.seen = Some(now);
        out
    }
}

// ----------------------------------------------------------------------------- delivery

/// `notify-send` arguments (libnotify >= 0.8: `--action` + `--wait` print the clicked action).
#[must_use]
pub fn notify_send_args(n: &OsNotice) -> Vec<String> {
    let mut a = vec![
        "--app-name=Pando".to_owned(),
        "--icon=dev.pando.assistant".to_owned(),
        "--expire-time=10000".to_owned(),
    ];
    if n.cell.is_some() {
        a.push("--action=default=Open".to_owned());
        a.push("--wait".to_owned());
    }
    a.push("--".to_owned());
    a.push(n.title.clone());
    a.push(n.body.clone());
    a
}

fn applescript_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `osascript` arguments for macOS.
#[must_use]
pub fn osascript_args(n: &OsNotice) -> Vec<String> {
    vec![
        "-e".to_owned(),
        format!(
            "display notification {} with title {}",
            applescript_quote(&n.body),
            applescript_quote(&n.title)
        ),
    ]
}

/// PowerShell script that shows a toast on Windows 10+.
#[must_use]
pub fn powershell_script(n: &OsNotice) -> String {
    let esc = |s: &str| {
        s.replace('\'', "''")
            .replace('&', "&amp;")
            .replace('<', "&lt;")
    };
    format!(
        "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null;\
$t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);\
$x = $t.GetElementsByTagName('text');\
$x.Item(0).AppendChild($t.CreateTextNode('{}')) > $null;\
$x.Item(1).AppendChild($t.CreateTextNode('{}')) > $null;\
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Pando').Show([Windows.UI.Notifications.ToastNotification]::new($t))",
        esc(&n.title),
        esc(&n.body)
    )
}

/// Shows `n` through the platform's mechanism. Never blocks the caller.
pub fn show(n: OsNotice, inbox: Sender<Msg>) {
    let _ = std::thread::Builder::new()
        .name("pando-notify".into())
        .spawn(move || {
            if cfg!(target_os = "linux") {
                let out = Process::new("notify-send")
                    .args(notify_send_args(&n))
                    .stdin(Stdio::null())
                    .stderr(Stdio::null())
                    .output();
                if let (Ok(out), Some(cell)) = (out, n.cell.clone())
                    && String::from_utf8_lossy(&out.stdout).trim() == "default"
                {
                    let _ = inbox.send_blocking(Msg::Command(Command::FocusCell { id: cell }));
                }
            } else if cfg!(target_os = "macos") {
                // No click callback without a bundle identity (pending): the notification is shown.
                let _ = Process::new("osascript").args(osascript_args(&n)).output();
            } else {
                let _ = Process::new("powershell")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        &powershell_script(&n),
                    ])
                    .output();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use organism::Engine;

    fn en() {
        rust_i18n::set_locale("en");
    }

    fn cell(id: &str, status: Status) -> CellInput {
        let mut c = CellInput::new(id, Engine::Pando, status);
        c.label = id.into();
        c
    }

    fn done_for(id: &str) -> HashSet<String> {
        std::iter::once(id.to_owned()).collect()
    }

    #[test]
    fn toasts_map_level_source_and_ttl() {
        let n = Notification {
            level: "warn".into(),
            source: "llm_provider".into(),
            message: " rate limited ".into(),
            ttl: 5_000_000_000,
            ..Notification::default()
        };
        assert_eq!(
            toast_for(&n),
            Some(ToastSpec {
                level: ToastLevel::Warning,
                message: "llm_provider: rate limited".into(),
                seconds: Some(5)
            })
        );
        let sticky = Notification {
            message: "x".into(),
            source: "system".into(),
            level: "error".into(),
            ..Notification::default()
        };
        let t = toast_for(&sticky).expect("toast");
        assert_eq!(
            (t.level, t.seconds, t.message.as_str()),
            (ToastLevel::Error, None, "x")
        );
        assert_eq!(toast_for(&Notification::default()), None);
    }

    #[test]
    fn scenario_agent_finished_while_unfocused() {
        en();
        let ev = NotifyEvent::AgentFinished {
            cell: "c1".into(),
            label: "docs".into(),
            failed: false,
        };
        // Unfocused and the user asked to be told: a notification that focuses the cell.
        let n = decide(&ev, true, false, &done_for("c1")).expect("notice");
        assert_eq!(n.cell.as_deref(), Some("c1"));
        assert!(n.body.contains("docs") && !n.title.starts_with("shell."));
        // Focused, disabled, or not requested: nothing.
        assert!(decide(&ev, true, true, &done_for("c1")).is_none());
        assert!(decide(&ev, false, false, &done_for("c1")).is_none());
        assert!(decide(&ev, true, false, &HashSet::new()).is_none());
    }

    #[test]
    fn waiting_and_connection_lost_notify_without_a_request() {
        en();
        let w = NotifyEvent::AgentWaiting {
            cell: "c".into(),
            label: "x".into(),
        };
        assert!(decide(&w, true, false, &HashSet::new()).is_some());
        let l = decide(&NotifyEvent::ConnectionLost, true, false, &HashSet::new()).expect("lost");
        assert!(l.cell.is_none());
        assert!(decide(&NotifyEvent::ConnectionLost, true, true, &HashSet::new()).is_none());
    }

    #[test]
    fn watcher_reports_transitions_only() {
        let mut w = CellWatcher::new();
        assert!(
            w.observe(&[cell("a", Status::Waiting)]).is_empty(),
            "first list only seeds"
        );
        assert!(w.observe(&[cell("a", Status::Waiting)]).is_empty());
        let ev = w.observe(&[cell("a", Status::Done), cell("b", Status::Waiting)]);
        assert_eq!(
            ev,
            vec![
                NotifyEvent::AgentFinished {
                    cell: "a".into(),
                    label: "a".into(),
                    failed: false
                },
                NotifyEvent::AgentWaiting {
                    cell: "b".into(),
                    label: "b".into()
                },
            ]
        );
        // Terminal to terminal (Done -> Error) is not a new "finished".
        assert!(
            w.observe(&[cell("a", Status::Error), cell("b", Status::Waiting)])
                .is_empty()
        );
        w.reset();
        assert!(w.observe(&[cell("z", Status::Waiting)]).is_empty());
    }

    #[test]
    fn delivery_commands() {
        let n = OsNotice {
            title: "T".into(),
            body: "B \"q\"".into(),
            cell: Some("c".into()),
        };
        let a = notify_send_args(&n);
        assert!(
            a.contains(&"--action=default=Open".to_owned()) && a.contains(&"--wait".to_owned())
        );
        assert_eq!(&a[a.len() - 2..], ["T", "B \"q\""]);
        let no_cell = notify_send_args(&OsNotice {
            cell: None,
            ..n.clone()
        });
        assert!(!no_cell.contains(&"--wait".to_owned()));
        assert!(
            osascript_args(&n)[1].contains(r#"with title "T""#)
                && osascript_args(&n)[1].contains(r#"\"q\""#)
        );
        assert!(
            powershell_script(&OsNotice {
                title: "It's".into(),
                ..n
            })
            .contains("It''s")
        );
    }
}
