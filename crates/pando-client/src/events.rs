//! Notification and config event streams, and the decision-model configuration.

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::PandoClient;
use crate::error::Result;
use crate::sse::SseEvent;
use crate::stream::{EventStream, EventStreamConfig, StreamItem};

/// A toast from `/notifications/stream`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Notification {
    /// Id.
    pub id: String,
    /// RFC 3339.
    pub time: String,
    /// `info|warn|error`.
    pub level: String,
    /// `llm_provider|tool|lsp|agent|system`.
    pub source: String,
    /// Text.
    pub message: String,
    /// Display time in nanoseconds; 0 = sticky.
    pub ttl: i64,
}

/// Event of the notifications stream.
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationEvent {
    /// Handshake.
    Connected,
    /// A toast.
    Notification(Notification),
    /// Anything else.
    Unknown {
        /// Event name.
        event: String,
        /// Raw data.
        raw: String,
    },
}

impl NotificationEvent {
    /// Decodes a frame.
    pub fn from_sse(ev: &SseEvent) -> Self {
        match ev.event.as_str() {
            "connected" => Self::Connected,
            "notification" => serde_json::from_str(&ev.data).map_or_else(
                |_| Self::Unknown {
                    event: ev.event.clone(),
                    raw: ev.data.clone(),
                },
                Self::Notification,
            ),
            _ => Self::Unknown {
                event: ev.event.clone(),
                raw: ev.data.clone(),
            },
        }
    }
}

/// A change on `/config/events`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ConfigEvent {
    /// Section that changed (`decisionModel`, `agents`, ...); empty = full reload.
    pub section: String,
    /// Timestamp.
    pub timestamp: String,
    /// `tui|webui|file|overlay`.
    pub source: String,
    /// Extra event kind.
    pub event: Option<String>,
}

/// Event of the config stream.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigStreamEvent {
    /// Handshake (`event: connected`).
    Connected,
    /// A change.
    Changed(ConfigEvent),
    /// Anything else.
    Unknown(String),
}

impl ConfigStreamEvent {
    /// Decodes a frame (changes are unnamed).
    pub fn from_sse(ev: &SseEvent) -> Self {
        if ev.event == "connected" {
            return Self::Connected;
        }
        serde_json::from_str(&ev.data)
            .map_or_else(|_| Self::Unknown(ev.data.clone()), Self::Changed)
    }
}

/// Router part of the decision-model config.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DecisionRouter {
    /// `ollama|typesafe|custom`.
    pub provider: String,
    /// Configured base URL.
    #[serde(rename = "baseURL")]
    pub base_url: String,
    /// Effective base URL.
    #[serde(rename = "effectiveBaseURL")]
    pub effective_base_url: String,
    /// Model.
    pub model: String,
    /// An API key is set (the key itself is never returned).
    pub api_key_set: bool,
    /// Masked key.
    pub api_key_masked: String,
}

/// `GET /config/decision-model`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DecisionModelConfig {
    /// Router.
    pub router: DecisionRouter,
    /// Timeout in ms; 0 = 1500 for ollama and 3000 for remote.
    pub timeout_ms: i64,
    /// Server warnings.
    pub warnings: Vec<String>,
}

/// `HealthReport` of the router.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HealthReport {
    /// Everything fine.
    pub ok: bool,
    /// Kind.
    pub kind: String,
    /// Model.
    pub model: String,
    /// Reachable.
    pub reachable: bool,
    /// Authorized.
    pub authorized: bool,
    /// Version acceptable.
    #[serde(rename = "versionOK")]
    pub version_ok: bool,
    /// Model present.
    pub model_present: bool,
    /// Latency in ms.
    pub latency_ms: i64,
    /// Problems.
    pub problems: Vec<String>,
}

/// `GET /decision-model/router/health` (cached 60 s server-side).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct RouterHealth {
    /// Healthy.
    pub ok: bool,
    /// Report.
    pub report: HealthReport,
    /// Problems.
    pub problems: Vec<String>,
}

impl PandoClient {
    /// `/notifications/stream`, self-reconnecting.
    pub fn notifications(&self, config: EventStreamConfig) -> EventStream<NotificationEvent> {
        EventStream::spawn(self.clone(), "/api/v1/notifications/stream", config, |e| {
            Some(NotificationEvent::from_sse(e))
        })
    }

    /// `/config/events`, self-reconnecting.
    pub fn config_events(&self, config: EventStreamConfig) -> EventStream<ConfigStreamEvent> {
        EventStream::spawn(self.clone(), "/api/v1/config/events", config, |e| {
            Some(ConfigStreamEvent::from_sse(e))
        })
    }

    /// `GET /config/decision-model`.
    pub async fn decision_model_config(&self) -> Result<DecisionModelConfig> {
        self.get_json("/api/v1/config/decision-model").await
    }

    /// `GET /decision-model/router/health`.
    pub async fn decision_model_health(&self) -> Result<RouterHealth> {
        self.get_json("/api/v1/decision-model/router/health").await
    }

    /// Reloads the decision-model config whenever Pando says it (or everything) changed.
    /// Yields the initial config, then one item per change. Dropping it stops the watcher.
    pub fn watch_decision_model(&self, config: EventStreamConfig) -> DecisionModelWatcher {
        let (tx, rx) = mpsc::channel(8);
        let client = self.clone();
        let task = tokio::spawn(async move {
            let mut events = client.config_events(config);
            while let Some(item) = events.next().await {
                let reload = match item {
                    // (Re)connected: reload, in case a change happened while we were away.
                    StreamItem::Connected => true,
                    StreamItem::Event(ConfigStreamEvent::Changed(c)) => {
                        matches!(c.section.as_str(), "" | "decisionModel" | "modelAutoMode")
                    }
                    _ => false,
                };
                if reload {
                    let cfg = client.decision_model_config().await;
                    if tx.send(cfg).await.is_err() {
                        return;
                    }
                }
            }
        });
        DecisionModelWatcher { rx, task }
    }
}

/// See [`PandoClient::watch_decision_model`].
#[derive(Debug)]
pub struct DecisionModelWatcher {
    rx: mpsc::Receiver<Result<DecisionModelConfig>>,
    task: JoinHandle<()>,
}

impl DecisionModelWatcher {
    /// The next (re)loaded config, or the error that prevented loading it.
    pub async fn next(&mut self) -> Option<Result<DecisionModelConfig>> {
        self.rx.recv().await
    }
}

impl Drop for DecisionModelWatcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}
