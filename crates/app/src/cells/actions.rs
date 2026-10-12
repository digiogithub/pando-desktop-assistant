//! Cell actions against Pando (PDA-T-0059, PDA-T-0060): cancel, delete, relaunch, create, and
//! delivery of approval answers.
//!
//! Approvals are never sent from here directly: the UI resolves a request in the fail-closed
//! `pando_client::approvals::Approvals` ledger with `Origin::Explicit`, and this module only
//! delivers what the ledger put in its outbox.

use pando_client::api::{CreateTask, TaskResponse};
use pando_client::approvals::Outgoing;
use pando_client::{PandoClient, Result};

/// The body that recreates `t`.
#[must_use]
pub fn relaunch_request(t: &TaskResponse) -> CreateTask {
    CreateTask {
        prompt: t.prompt.clone(),
        work_dir: t.work_dir.clone().filter(|w| !w.is_empty()),
        model: Some(t.model.clone()).filter(|m| !m.is_empty()),
        engine: Some(t.agent.clone()).filter(|a| !a.is_empty()),
        tags: t.tags.clone().unwrap_or_default(),
        dependencies: Vec::new(),
        background: true,
    }
}

/// The action a failure belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    /// Cancel a task.
    Cancel,
    /// Delete a task.
    Delete,
    /// Relaunch or create a task.
    Create,
}

/// How a failed call is reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// 409 on cancel: the task had already finished (an informational toast, not an error).
    AlreadyFinished,
    /// 404: the task is gone.
    Gone,
    /// Anything else, with the (secret-free) error text.
    Failed(String),
}

/// Classifies an error so the UI can say something specific.
#[must_use]
pub fn classify(kind: ActionKind, err: &pando_client::Error) -> Failure {
    match err {
        pando_client::Error::Server { status: 409, .. } if kind == ActionKind::Cancel => {
            Failure::AlreadyFinished
        }
        pando_client::Error::Server { status: 404, .. } => Failure::Gone,
        other => Failure::Failed(other.to_string()),
    }
}

/// `POST /orchestrator/tasks/{id}/cancel`.
pub async fn cancel(client: &PandoClient, id: &str) -> Result<()> {
    client.cancel_task(id).await.map(|_| ())
}

/// `DELETE /orchestrator/tasks/{id}`.
pub async fn delete(client: &PandoClient, id: &str) -> Result<()> {
    client.delete_task(id).await
}

/// Creates the same task again.
pub async fn relaunch(client: &PandoClient, t: &TaskResponse) -> Result<TaskResponse> {
    client.create_task(&relaunch_request(t)).await
}

/// Creates a task from the form.
pub async fn create(client: &PandoClient, body: &CreateTask) -> Result<TaskResponse> {
    client.create_task(body).await
}

/// Sends ledger answers to Pando; returns the ids that got through (the caller marks them
/// delivered, failures stay in the outbox for the next call).
pub async fn deliver(client: &PandoClient, outbox: Vec<Outgoing>) -> Vec<String> {
    let mut delivered = Vec::new();
    for o in outbox {
        let (id, ok) = match &o {
            Outgoing::Permission {
                id,
                session_id,
                action,
            } => (
                id.clone(),
                client
                    .permissions_respond(id, session_id, *action)
                    .await
                    .is_ok(),
            ),
            Outgoing::Question {
                id,
                session_id,
                answers,
                cancelled,
            } => (
                id.clone(),
                client
                    .questions_respond(id, session_id, answers, *cancelled)
                    .await
                    .is_ok(),
            ),
        };
        if ok {
            delivered.push(id);
        }
    }
    delivered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_on_cancel_is_not_a_generic_error() {
        let conflict = pando_client::Error::Server {
            status: 409,
            message: "task already terminal".into(),
        };
        assert_eq!(
            classify(ActionKind::Cancel, &conflict),
            Failure::AlreadyFinished
        );
        assert!(matches!(
            classify(ActionKind::Delete, &conflict),
            Failure::Failed(_)
        ));
        let gone = pando_client::Error::Server {
            status: 404,
            message: String::new(),
        };
        assert_eq!(classify(ActionKind::Delete, &gone), Failure::Gone);
        assert!(matches!(
            classify(ActionKind::Create, &pando_client::Error::Timeout),
            Failure::Failed(_)
        ));
    }

    #[test]
    fn relaunch_copies_the_task_definition() {
        let t = TaskResponse {
            id: "old".into(),
            prompt: "run tests".into(),
            agent: "acp-claude".into(),
            model: "sonnet".into(),
            work_dir: Some("/w".into()),
            tags: Some(vec!["ci".into()]),
            ..TaskResponse::default()
        };
        let r = relaunch_request(&t);
        assert_eq!(r.prompt, "run tests");
        assert_eq!(r.engine.as_deref(), Some("acp-claude"));
        assert_eq!(r.model.as_deref(), Some("sonnet"));
        assert_eq!(r.work_dir.as_deref(), Some("/w"));
        assert_eq!(r.tags, ["ci"]);
        let bare = relaunch_request(&TaskResponse::default());
        assert_eq!((bare.engine, bare.model, bare.work_dir), (None, None, None));
    }
}
