//! Transport trait and the System One client built on it.
//!
//! `decide` has no network dependency (`cargo xtask check-deps`): it encodes, validates
//! and decodes `/v1/systemone` bodies, and delegates the HTTP round trip to a
//! [`DecisionTransport`] implemented elsewhere (the app, over `reqwest` behind the GPUI
//! tokio bridge). The transport owns the endpoint: `{effectiveBaseURL}/v1/systemone`
//! from `GET /api/v1/config/decision-model`, the `Authorization: Bearer` header when a
//! key is available from the environment, extra headers, TLS — or, later, a Pando proxy.

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;
use std::time::{Duration, Instant};

use crate::error::DecisionError;
use crate::protocol::{Request, Response};

/// Default per-call deadline of the assistant (`PDA-SP-0008.R4`: > 400 ms is
/// "unavailable"). Deliberately tighter than Pando's 1500/3000 ms defaults.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(400);

/// One encoded request handed to the transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRequest {
    /// JSON body for `POST …/v1/systemone`, already validated and ≤ 64 KiB.
    pub body: Vec<u8>,
    /// Deadline for the whole round trip; the transport must enforce it.
    pub timeout: Duration,
}

/// Raw HTTP answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportResponse {
    /// HTTP status.
    pub status: u16,
    /// Body (the transport may cap it, e.g. at 4 MiB).
    pub body: Vec<u8>,
}

/// Failure before an HTTP status was received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// Connection refused, DNS, TLS... Messages must not contain secrets.
    Unreachable(String),
    /// The deadline elapsed.
    Timeout,
}

/// Performs the HTTP round trip of one System One call. Implementations never retry.
pub trait DecisionTransport: Send + Sync {
    /// `POST {base}/v1/systemone` with `Content-Type: application/json`.
    fn post(
        &self,
        request: TransportRequest,
    ) -> impl Future<Output = Result<TransportResponse, TransportError>> + Send;
}

impl<T: DecisionTransport + ?Sized> DecisionTransport for Arc<T> {
    fn post(
        &self,
        request: TransportRequest,
    ) -> impl Future<Output = Result<TransportResponse, TransportError>> + Send {
        (**self).post(request)
    }
}

/// Model-level options that go into every request body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientOptions {
    /// Decision model, e.g. `tev1:0.8b` (from `/config/decision-model`).
    pub model: String,
    /// Ollama `keep_alive`; `None` omits it (remote gateways).
    pub keep_alive: Option<String>,
    /// Per-call deadline; [`DEFAULT_TIMEOUT`] by default.
    pub timeout: Duration,
    /// Model context in tokens, used to budget the state (`tev1:0.8b`: 2050).
    pub context_tokens: usize,
}

impl ClientOptions {
    /// Options for `model` with the assistant defaults.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            keep_alive: Some("30m".into()),
            timeout: DEFAULT_TIMEOUT,
            context_tokens: 2050,
        }
    }
}

/// A decoded answer and how long it took.
#[derive(Debug, Clone, PartialEq)]
pub struct Timed<T> {
    /// The value.
    pub value: T,
    /// Wall-clock latency of the round trip.
    pub latency: Duration,
}

/// System One client over a [`DecisionTransport`]. Never retries.
#[derive(Debug, Clone)]
pub struct SystemOneClient<T> {
    transport: T,
    options: ClientOptions,
}

impl<T: DecisionTransport> SystemOneClient<T> {
    /// Build a client.
    pub fn new(transport: T, options: ClientOptions) -> Self {
        Self { transport, options }
    }

    /// The options.
    pub fn options(&self) -> &ClientOptions {
        &self.options
    }

    /// Build a request for the configured model.
    pub fn request(
        &self,
        state: String,
        questions: Vec<(String, crate::protocol::Question)>,
    ) -> Request {
        Request {
            model: self.options.model.clone(),
            state,
            questions,
            keep_alive: self.options.keep_alive.clone(),
        }
    }

    /// Send one request: validate, encode (64 KiB cap), post, map the status, decode
    /// and check every answer against its question.
    pub async fn decide(&self, request: &Request) -> Result<Timed<Response>, DecisionError> {
        let body = request.encode()?;
        let started = Instant::now();
        let raw = self
            .transport
            .post(TransportRequest {
                body,
                timeout: self.options.timeout,
            })
            .await
            .map_err(|e| match e {
                TransportError::Unreachable(m) => DecisionError::Unreachable(m),
                TransportError::Timeout => DecisionError::Timeout,
            })?;
        let latency = started.elapsed();
        if !(200..300).contains(&raw.status) {
            return Err(DecisionError::from_status(raw.status, &raw.body));
        }
        let value = Response::decode_checked(&raw.body, request)?;
        Ok(Timed { value, latency })
    }
}

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Minimal single-future executor (park/unpark). For tests, the eval CLI and
/// synchronous transports; the app awaits the futures on its own executor instead.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
pub(crate) mod mock {
    //! Scriptable transport that replays Ollama-style answers and errors.

    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    /// Scripted reply.
    #[derive(Debug, Clone)]
    pub enum Reply {
        Ok(String),
        Status(u16, String),
        Fail(TransportError),
        /// Answer each question with the given (choice, p) pairs, generated from the
        /// request so criteria stay consistent.
        Answer(Vec<(&'static str, &'static str, f64)>),
    }

    /// Records requests and pops scripted replies.
    #[derive(Debug, Default)]
    pub struct MockTransport {
        pub replies: Mutex<VecDeque<Reply>>,
        pub seen: Mutex<Vec<TransportRequest>>,
    }

    impl MockTransport {
        pub fn new(replies: Vec<Reply>) -> Self {
            Self {
                replies: Mutex::new(replies.into()),
                seen: Mutex::default(),
            }
        }

        pub fn seen_json(&self) -> Vec<serde_json::Value> {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .map(|r| serde_json::from_slice(&r.body).unwrap())
                .collect()
        }
    }

    /// Build an Ollama-shaped answer body: the chosen key gets `p`, the rest share
    /// `1 - p` evenly.
    pub fn answer_body(req: &serde_json::Value, picks: &[(&str, &str, f64)]) -> String {
        let mut answers = serde_json::Map::new();
        for (name, choice, p) in picks {
            let keys: Vec<String> = req["questions"][*name]["criteria"]
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            let rest = (1.0 - p) / (keys.len().saturating_sub(1).max(1)) as f64;
            let probs: serde_json::Map<_, _> = keys
                .iter()
                .map(|k| {
                    let v = if k == choice { *p } else { rest };
                    (k.clone(), serde_json::json!(v))
                })
                .collect();
            answers.insert(
                (*name).to_string(),
                serde_json::json!({"type":"choice","choice":choice,"probabilities":probs,"confidence":p}),
            );
        }
        serde_json::json!({"model":"tev1:0.8b","answers":answers,"usage":{"input_tokens":100,"output_tokens":1}})
            .to_string()
    }

    impl DecisionTransport for MockTransport {
        async fn post(
            &self,
            request: TransportRequest,
        ) -> Result<TransportResponse, TransportError> {
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Reply::Fail(TransportError::Unreachable("no script".into())));
            let req_json: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            self.seen.lock().unwrap().push(request);
            match reply {
                Reply::Ok(b) => Ok(TransportResponse {
                    status: 200,
                    body: b.into_bytes(),
                }),
                Reply::Status(s, b) => Ok(TransportResponse {
                    status: s,
                    body: b.into_bytes(),
                }),
                Reply::Fail(e) => Err(e),
                Reply::Answer(picks) => Ok(TransportResponse {
                    status: 200,
                    body: answer_body(&req_json, &picks).into_bytes(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::{MockTransport, Reply};
    use super::*;
    use crate::protocol::{Criterion, Question};

    fn client(replies: Vec<Reply>) -> SystemOneClient<MockTransport> {
        SystemOneClient::new(MockTransport::new(replies), ClientOptions::new("tev1:0.8b"))
    }

    fn intent(c: &SystemOneClient<MockTransport>) -> Request {
        c.request(
            "{}".into(),
            vec![(
                "intent".into(),
                Question::choice(
                    "i",
                    vec![
                        Criterion::new("control", ""),
                        Criterion::new("conversation", ""),
                    ],
                ),
            )],
        )
    }

    #[test]
    fn happy_path_sends_model_keep_alive_and_timeout() {
        let c = client(vec![Reply::Answer(vec![("intent", "control", 0.9)])]);
        let r = block_on(c.decide(&intent(&c))).unwrap();
        assert_eq!(r.value.answers["intent"].p_top(), Some(0.9));
        assert_eq!(
            c.transport.seen.lock().unwrap()[0].timeout,
            Duration::from_millis(400)
        );
        let body = &c.transport.seen_json()[0];
        assert_eq!(body["model"], "tev1:0.8b");
        assert_eq!(body["keep_alive"], "30m");
    }

    #[test]
    fn typed_errors() {
        let cases: Vec<(Reply, &str)> = vec![
            (Reply::Fail(TransportError::Timeout), "timeout"),
            (
                Reply::Fail(TransportError::Unreachable("refused".into())),
                "unreachable",
            ),
            (Reply::Status(401, "{}".into()), "unauthorized"),
            (
                Reply::Status(404, r#"{"error":"model \"x\" not found"}"#.into()),
                "model_not_found",
            ),
            (Reply::Status(413, String::new()), "too_large"),
            (Reply::Status(429, String::new()), "rate_limited"),
            (Reply::Status(500, "boom".into()), "server"),
            (Reply::Ok("not json".into()), "malformed"),
            (
                Reply::Ok(r#"{"answers":{"intent":{"choice":"nope"}}}"#.into()),
                "malformed",
            ),
        ];
        for (reply, kind) in cases {
            let c = client(vec![reply]);
            let err = block_on(c.decide(&intent(&c))).unwrap_err();
            assert_eq!(err.kind(), kind, "{err}");
        }
    }

    #[test]
    fn invalid_requests_never_reach_the_transport() {
        let c = client(vec![]);
        let mut r = intent(&c);
        r.state = "x".repeat(70_000);
        assert_eq!(block_on(c.decide(&r)).unwrap_err().kind(), "too_large");
        assert!(c.transport.seen.lock().unwrap().is_empty());
    }
}
