//! Generic reconnecting event stream for the long-lived SSE feeds of Pando
//! (`/notifications/stream`, `/config/events`, `/projects/events`, ...).
//!
//! Pando sends no keep-alives and no event ids today, so a silent stream is normal. The
//! optional watchdog (`idle_timeout`) is meant to be combined with polling by the caller.
//! `Last-Event-ID` is sent on reconnect when the server ever supplied an `id:` (gap G9).

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::PandoClient;
use crate::error::Error;
use crate::sse::SseEvent;

/// Reconnection tuning.
#[derive(Debug, Clone)]
pub struct EventStreamConfig {
    /// First reconnect delay.
    pub backoff_min: Duration,
    /// Reconnect delay cap (delay doubles from `backoff_min`).
    pub backoff_max: Duration,
    /// Reconnect when nothing arrives for this long. `None` disables the watchdog.
    pub idle_timeout: Option<Duration>,
}

impl Default for EventStreamConfig {
    fn default() -> Self {
        Self {
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
            idle_timeout: None,
        }
    }
}

/// What an [`EventStream`] yields.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamItem<T> {
    /// The connection (re)opened.
    Connected,
    /// A decoded event.
    Event(T),
    /// The connection ended or failed; a reconnect follows after the backoff.
    Disconnected {
        /// Why (never contains secrets).
        reason: String,
    },
}

/// A self-reconnecting stream decoded into `T`. Dropping it stops the background task.
#[derive(Debug)]
pub struct EventStream<T> {
    rx: mpsc::Receiver<StreamItem<T>>,
    task: JoinHandle<()>,
}

impl<T: Send + 'static> EventStream<T> {
    /// Starts streaming `GET path`. `decode` maps each frame to `T` (return `None` to skip it,
    /// or keep unknown frames in `T` as an `Unknown{raw}` variant).
    pub fn spawn<F>(client: PandoClient, path: &str, config: EventStreamConfig, decode: F) -> Self
    where
        F: Fn(&SseEvent) -> Option<T> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(256);
        let path = path.to_owned();
        let task = tokio::spawn(async move {
            let mut backoff = config.backoff_min;
            let mut last_id: Option<String> = None;
            loop {
                let headers: Vec<(&str, &str)> = last_id
                    .as_deref()
                    .map(|id| vec![("Last-Event-ID", id)])
                    .unwrap_or_default();
                let reason = match client.open_sse_with::<()>(&path, None, &headers).await {
                    Err(e) => describe(&e),
                    Ok(mut stream) => {
                        if tx.send(StreamItem::Connected).await.is_err() {
                            return;
                        }
                        loop {
                            let next = match config.idle_timeout {
                                Some(d) => match tokio::time::timeout(d, stream.next_event()).await
                                {
                                    Ok(r) => r,
                                    Err(_) => break "idle timeout".to_owned(),
                                },
                                None => stream.next_event().await,
                            };
                            match next {
                                Ok(Some(ev)) => {
                                    backoff = config.backoff_min;
                                    if let Some(id) = &ev.id {
                                        last_id = Some(id.clone());
                                    }
                                    if let Some(item) = decode(&ev)
                                        && tx.send(StreamItem::Event(item)).await.is_err()
                                    {
                                        return;
                                    }
                                }
                                Ok(None) => break "stream closed".to_owned(),
                                Err(e) => break describe(&e),
                            }
                        }
                    }
                };
                if tx.send(StreamItem::Disconnected { reason }).await.is_err() {
                    return;
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(config.backoff_max);
            }
        });
        Self { rx, task }
    }

    /// The next item; `None` only after [`EventStream::close`].
    pub async fn next(&mut self) -> Option<StreamItem<T>> {
        self.rx.recv().await
    }

    /// Stops the stream.
    pub fn close(&mut self) {
        self.task.abort();
        self.rx.close();
    }
}

impl<T> Drop for EventStream<T> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn describe(e: &Error) -> String {
    e.to_string()
}
