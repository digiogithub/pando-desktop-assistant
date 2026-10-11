//! Orchestrator/projects/models types and the project-proxy router.

mod common;

use axum::Router;
use axum::http::HeaderMap;
use axum::routing::get;
use common::spawn_http;
use pando_client::api::{TaskChange, TaskList, TaskTracker};
use pando_client::{PandoClient, PandoConfig};

#[test]
fn old_and_new_task_shapes_both_parse() {
    let old = r#"{"tasks":[{"id":"t1","name":"n","prompt":"p","agent":"claude","model":"m","persona":"","status":"running","progress":40,"tokens":0,"output":"","current_tool":"bash","tool_calls":null,"created_at":"2026-10-12T00:00:00Z","updated_at":"2026-10-12T00:00:01Z"}],"total":1}"#;
    let l: TaskList = serde_json::from_str(old).unwrap_or_else(|e| panic!("{e}"));
    let t = &l.tasks[0];
    assert_eq!(t.status, "running");
    assert!(t.conclusion.is_none() && t.project_id.is_none() && t.acp_status.is_none());

    let new = r#"{"tasks":[{"id":"t2","status":"completed","project_id":"p1","project_name":"demo","parent_session_id":"s1","error":"","exit_code":0,"acp_status":{"session_id":"a","is_connected":true},"conclusion":{"status":"success","summary":"done","artifacts":["a.go"],"confidence":0.9,"captured_at":"2026-10-12T00:00:00Z"}}],"total":1}"#;
    let l: TaskList = serde_json::from_str(new).unwrap_or_else(|e| panic!("{e}"));
    let t = &l.tasks[0];
    assert!(t.is_terminal());
    assert_eq!(t.project_name.as_deref(), Some("demo"));
    assert_eq!(
        t.conclusion.as_ref().map(|c| c.summary.as_str()),
        Some("done")
    );
    assert!(t.acp_status.as_ref().is_some_and(|a| a.is_connected));
}

#[test]
fn recorded_orchestrator_fixture_parses() {
    let l: TaskList = serde_json::from_str(include_str!("fixtures/orchestrator_tasks.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(l.total, 0);
}

#[test]
fn tracker_reports_births_updates_deaths_and_removals() {
    let mk = |id: &str, status: &str, progress: i64| pando_client::api::TaskResponse {
        id: id.into(),
        status: status.into(),
        progress,
        ..Default::default()
    };
    let mut tr = TaskTracker::new();
    let c = tr.update(&[mk("a", "running", 0)]);
    assert!(matches!(c.as_slice(), [TaskChange::Appeared(_)]));
    assert!(tr.update(&[mk("a", "running", 0)]).is_empty());
    let c = tr.update(&[mk("a", "running", 50), mk("b", "pending", 0)]);
    assert!(matches!(
        c.as_slice(),
        [TaskChange::Updated(_), TaskChange::Appeared(_)]
    ));
    let c = tr.update(&[mk("a", "completed", 100)]);
    assert!(
        matches!(c.as_slice(), [TaskChange::Finished(t), TaskChange::Removed(r)] if t.id == "a" && r == "b")
    );
}

#[tokio::test]
async fn project_proxy_rewrites_the_path_and_keeps_headers() {
    let app = Router::new()
        .route(
            "/api/v1/projects/p1/web/api/v1/sessions",
            get(|h: HeaderMap| async move {
                format!(
                    r#"{{"client":"{}","token":"{}"}}"#,
                    h.get("x-pando-client")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or(""),
                    h.get("x-pando-token")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                )
            }),
        )
        .route(
            "/api/v1/projects",
            get(|| async {
                r#"{"projects":[{"id":"p1","name":"demo","status":"running","web_port":9000}]}"#
            }),
        );
    let srv = spawn_http(app).await;
    let root = PandoClient::new(PandoConfig::new(srv.base_url()).with_token("tk"))
        .unwrap_or_else(|e| panic!("{e}"));
    let projects = root.list_projects().await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(projects[0].web_port, Some(9000));

    let child = root.project_client(&projects[0].id);
    let v: serde_json::Value = child
        .get_json("/api/v1/sessions")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v["client"], "pando-assistant");
    assert_eq!(v["token"], "tk");
}
