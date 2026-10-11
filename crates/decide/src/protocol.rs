//! Wire types of the System One protocol, `POST {base}/v1/systemone`.
//!
//! Mirrors Pando's `internal/llm/systemone/client.go` (see `docs/research/pando-api.md`
//! §5): `questions` is an object keyed by question name (1–64 entries) and a choice
//! question carries an **ordered** `criteria` object of 2–26 options whose order matters
//! to the model. Both are kept as ordered vectors here and encoded through
//! `serde_json::Map`, which preserves insertion order in this workspace
//! (`preserve_order`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::DecisionError;

/// Request body cap enforced by Ollama (and locally).
pub const MAX_BODY_BYTES: usize = 64 * 1024;
/// Maximum number of questions per request.
pub const MAX_QUESTIONS: usize = 64;
/// Minimum number of criteria of a choice question.
pub const MIN_CHOICE_CRITERIA: usize = 2;
/// Maximum number of criteria of a choice question.
pub const MAX_CHOICE_CRITERIA: usize = 26;

/// Question type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionType {
    /// Pick one of the ordered criteria.
    Choice,
    /// Yes / no.
    Noul,
    /// Numeric score.
    Score,
}

impl QuestionType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Choice => "choice",
            Self::Noul => "noul",
            Self::Score => "score",
        }
    }
}

/// One ordered option of a choice question. `None` description encodes as JSON `null`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    /// Option key, returned verbatim as `choice`.
    pub key: String,
    /// Optional description that helps the model.
    pub description: Option<String>,
}

impl Criterion {
    /// Build a criterion; an empty description becomes `None`.
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        let description = description.into();
        Self {
            key: key.into(),
            description: (!description.is_empty()).then_some(description),
        }
    }
}

/// One System One question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "Value", try_from = "Value")]
pub struct Question {
    /// Question type.
    pub kind: QuestionType,
    /// Instructions for the model (required by the server).
    pub instructions: String,
    /// Ordered criteria (choice questions only).
    pub criteria: Vec<Criterion>,
    /// Extra type-specific fields merged into the question object (e.g. score bounds).
    pub extra: Map<String, Value>,
}

impl Question {
    /// A choice question over ordered criteria.
    pub fn choice(instructions: impl Into<String>, criteria: Vec<Criterion>) -> Self {
        Self {
            kind: QuestionType::Choice,
            instructions: instructions.into(),
            criteria,
            extra: Map::new(),
        }
    }

    /// Keys of the criteria, in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.criteria.iter().map(|c| c.key.as_str())
    }
}

impl From<Question> for Value {
    fn from(q: Question) -> Self {
        let mut m = Map::new();
        m.insert("type".into(), Value::String(q.kind.as_str().into()));
        m.insert("instructions".into(), Value::String(q.instructions));
        if !q.criteria.is_empty() {
            let mut c = Map::new();
            for crit in q.criteria {
                c.insert(
                    crit.key,
                    crit.description.map_or(Value::Null, Value::String),
                );
            }
            m.insert("criteria".into(), Value::Object(c));
        }
        for (k, v) in q.extra {
            m.entry(k).or_insert(v);
        }
        Value::Object(m)
    }
}

impl TryFrom<Value> for Question {
    type Error = String;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut m) = v else {
            return Err("question must be an object".into());
        };
        let kind: QuestionType = serde_json::from_value(m.remove("type").unwrap_or(Value::Null))
            .map_err(|e| format!("question type: {e}"))?;
        let instructions = match m.remove("instructions") {
            Some(Value::String(s)) => s,
            Some(other) => other.to_string(),
            None => String::new(),
        };
        let criteria = match m.remove("criteria") {
            Some(Value::Object(c)) => c
                .into_iter()
                .map(|(key, d)| Criterion {
                    key,
                    description: match d {
                        Value::String(s) => Some(s),
                        Value::Null => None,
                        other => Some(other.to_string()),
                    },
                })
                .collect(),
            Some(_) => return Err("criteria must be an object".into()),
            None => Vec::new(),
        };
        Ok(Self {
            kind,
            instructions,
            criteria,
            extra: m,
        })
    }
}

/// The System One request body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "Value", try_from = "Value")]
pub struct Request {
    /// Model name, e.g. `tev1:0.8b`.
    pub model: String,
    /// Shared state; the assistant (like Pando) sends a JSON-object string.
    pub state: String,
    /// Questions keyed by name, in order.
    pub questions: Vec<(String, Question)>,
    /// Ollama `keep_alive` (e.g. `30m`); omitted when `None`.
    pub keep_alive: Option<String>,
}

impl From<Request> for Value {
    fn from(r: Request) -> Self {
        let mut m = Map::new();
        m.insert("model".into(), Value::String(r.model));
        m.insert("state".into(), Value::String(r.state));
        let mut q = Map::new();
        for (name, question) in r.questions {
            q.insert(name, question.into());
        }
        m.insert("questions".into(), Value::Object(q));
        if let Some(k) = r.keep_alive {
            m.insert("keep_alive".into(), Value::String(k));
        }
        Value::Object(m)
    }
}

impl TryFrom<Value> for Request {
    type Error = String;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut m) = v else {
            return Err("request must be an object".into());
        };
        let model = match m.remove("model") {
            Some(Value::String(s)) => s,
            _ => return Err("model must be a string".into()),
        };
        let state = match m.remove("state") {
            Some(Value::String(s)) => s,
            Some(other) => other.to_string(),
            None => String::new(),
        };
        let questions = match m.remove("questions") {
            Some(Value::Object(q)) => q
                .into_iter()
                .map(|(k, v)| Question::try_from(v).map(|q| (k, q)))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err("questions must be an object".into()),
        };
        let keep_alive = match m.remove("keep_alive") {
            Some(Value::String(s)) => Some(s),
            _ => None,
        };
        Ok(Self {
            model,
            state,
            questions,
            keep_alive,
        })
    }
}

impl Request {
    /// Enforce the protocol bounds locally (Pando's `Request.validate`), plus unique
    /// question names and unique criterion keys, which an ordered JSON object needs.
    pub fn validate(&self) -> Result<(), DecisionError> {
        if self.model.trim().is_empty() {
            return Err(DecisionError::BadRequest("model is required".into()));
        }
        let n = self.questions.len();
        if !(1..=MAX_QUESTIONS).contains(&n) {
            return Err(DecisionError::BadRequest(format!(
                "questions must contain 1-{MAX_QUESTIONS} entries, got {n}"
            )));
        }
        let mut names = std::collections::BTreeSet::new();
        for (name, q) in &self.questions {
            if !names.insert(name.as_str()) {
                return Err(DecisionError::BadRequest(format!(
                    "duplicate question {name:?}"
                )));
            }
            if q.kind == QuestionType::Choice {
                let c = q.criteria.len();
                if !(MIN_CHOICE_CRITERIA..=MAX_CHOICE_CRITERIA).contains(&c) {
                    return Err(DecisionError::BadRequest(format!(
                        "question {name:?}: choice criteria must contain \
                         {MIN_CHOICE_CRITERIA}-{MAX_CHOICE_CRITERIA} candidates, got {c}"
                    )));
                }
                let mut keys = std::collections::BTreeSet::new();
                if let Some(dup) = q.keys().find(|k| !keys.insert(*k)) {
                    return Err(DecisionError::BadRequest(format!(
                        "question {name:?}: duplicate criterion {dup:?}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Validate and encode the body, enforcing the 64 KiB cap.
    pub fn encode(&self) -> Result<Vec<u8>, DecisionError> {
        self.validate()?;
        let body = serde_json::to_vec(self)
            .map_err(|e| DecisionError::BadRequest(format!("cannot encode request: {e}")))?;
        if body.len() > MAX_BODY_BYTES {
            return Err(DecisionError::TooLarge(format!(
                "request body is {} bytes, limit is {MAX_BODY_BYTES}",
                body.len()
            )));
        }
        Ok(body)
    }

    /// The question with this name.
    pub fn question(&self, name: &str) -> Option<&Question> {
        self.questions
            .iter()
            .find_map(|(n, q)| (n == name).then_some(q))
    }
}

/// The decoded answer to one question.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Answer {
    /// Echoed question type.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Chosen criterion key (choice) or `yes`/`no` (noul).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<String>,
    /// Probability per criterion key.
    #[serde(default)]
    pub probabilities: BTreeMap<String, f64>,
    /// Model-reported confidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Score (score questions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

impl Answer {
    /// Probability of the chosen option, `p(top)`, like Pando's
    /// `ans.Probabilities[ans.Choice]`. `None` when the choice or its probability is
    /// missing, not finite or outside `[0, 1]`; the policy treats `None` as 0
    /// (fail-closed).
    pub fn p_top(&self) -> Option<f64> {
        let p = *self.probabilities.get(self.choice.as_deref()?)?;
        (p.is_finite() && (0.0..=1.0).contains(&p)).then_some(p)
    }

    /// Options sorted by probability, descending; ties keep key order. Non-finite
    /// probabilities are dropped.
    pub fn ranked(&self) -> Vec<(String, f64)> {
        let mut v: Vec<(String, f64)> = self
            .probabilities
            .iter()
            .filter(|(_, p)| p.is_finite())
            .map(|(k, p)| (k.clone(), *p))
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }
}

/// Token usage and (on gateways) cost.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt tokens.
    #[serde(default)]
    pub input_tokens: u64,
    /// Output tokens.
    #[serde(default)]
    pub output_tokens: u64,
    /// Cost reported by a gateway.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
}

/// The System One response body.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Response {
    /// Model that answered.
    #[serde(default)]
    pub model: String,
    /// Answers keyed by question name.
    #[serde(default)]
    pub answers: BTreeMap<String, Answer>,
    /// Usage, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl Response {
    /// Decode a 2xx body and check it against the request (Pando's `checkAnswers`):
    /// every question has an answer and every choice is one of its criteria.
    pub fn decode_checked(body: &[u8], req: &Request) -> Result<Self, DecisionError> {
        let resp: Self = serde_json::from_slice(body)
            .map_err(|e| DecisionError::Malformed(format!("invalid JSON: {e}")))?;
        for (name, q) in &req.questions {
            let a = resp.answers.get(name).ok_or_else(|| {
                DecisionError::Malformed(format!("answer for question {name:?} is missing"))
            })?;
            if q.kind != QuestionType::Choice {
                continue;
            }
            let choice = a.choice.as_deref().unwrap_or_default();
            if !q.keys().any(|k| k == choice) {
                return Err(DecisionError::Malformed(format!(
                    "question {name:?}: choice {choice:?} is not among the criteria"
                )));
            }
        }
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent_request() -> Request {
        Request {
            model: "tev1:0.8b".into(),
            state: r#"{"transcript":"hola"}"#.into(),
            questions: vec![(
                "intent".into(),
                Question::choice(
                    "classify",
                    vec![
                        Criterion::new("control", "a command"),
                        Criterion::new("conversation", ""),
                        Criterion::new("dictation", "text to insert"),
                    ],
                ),
            )],
            keep_alive: Some("30m".into()),
        }
    }

    #[test]
    fn encodes_ordered_criteria_and_null_descriptions() {
        let body = intent_request().encode().unwrap();
        let s = String::from_utf8(body).unwrap();
        assert_eq!(
            s,
            r#"{"model":"tev1:0.8b","state":"{\"transcript\":\"hola\"}","questions":{"intent":{"type":"choice","instructions":"classify","criteria":{"control":"a command","conversation":null,"dictation":"text to insert"}}},"keep_alive":"30m"}"#
        );
    }

    #[test]
    fn criteria_order_is_preserved_even_when_unsorted() {
        let mut r = intent_request();
        r.questions[0].1.criteria.reverse();
        let s = String::from_utf8(r.encode().unwrap()).unwrap();
        let d = s.find("dictation").unwrap();
        let c = s.find("\"control\"").unwrap();
        assert!(d < c);
    }

    #[test]
    fn request_round_trips() {
        let r = intent_request();
        let v = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn validation_bounds() {
        let mut r = intent_request();
        r.model.clear();
        assert!(matches!(r.validate(), Err(DecisionError::BadRequest(_))));

        let mut r = intent_request();
        r.questions.clear();
        assert!(matches!(r.validate(), Err(DecisionError::BadRequest(_))));

        let mut r = intent_request();
        r.questions[0].1.criteria.truncate(1);
        assert!(matches!(r.validate(), Err(DecisionError::BadRequest(_))));

        let mut r = intent_request();
        r.questions[0].1.criteria = (0..27).map(|i| Criterion::new(i.to_string(), "")).collect();
        assert!(matches!(r.validate(), Err(DecisionError::BadRequest(_))));

        let mut r = intent_request();
        r.questions[0].1.criteria[1].key = "control".into();
        assert!(matches!(r.validate(), Err(DecisionError::BadRequest(_))));

        let mut r = intent_request();
        let q = r.questions[0].clone();
        r.questions.push(q);
        assert!(matches!(r.validate(), Err(DecisionError::BadRequest(_))));

        let mut r = intent_request();
        let q = r.questions[0].1.clone();
        r.questions = (0..65).map(|i| (format!("q{i}"), q.clone())).collect();
        assert!(matches!(r.validate(), Err(DecisionError::BadRequest(_))));
    }

    #[test]
    fn body_cap_is_enforced() {
        let mut r = intent_request();
        r.state = "x".repeat(MAX_BODY_BYTES);
        assert!(matches!(r.encode(), Err(DecisionError::TooLarge(_))));
    }

    /// Recorded Ollama 0.35.0 answer (Pando `systemonetest.DuplicateChargeResponseJSON`).
    const OLLAMA_RECORDED: &str = r#"{"model":"tev1:0.8b","answers":{"intent":{"type":"choice","choice":"duplicate_charge","probabilities":{"duplicate_charge":0.9760968387170561,"refund_request":0.0155031612829439,"other":0.0084},"confidence":0.9145015492062412}},"usage":{"input_tokens":111,"output_tokens":1}}"#;

    #[test]
    fn decodes_recorded_ollama_answer_with_full_precision() {
        let req = Request {
            questions: vec![(
                "intent".into(),
                Question::choice(
                    "billing",
                    vec![
                        Criterion::new("duplicate_charge", ""),
                        Criterion::new("refund_request", ""),
                        Criterion::new("other", ""),
                    ],
                ),
            )],
            ..intent_request()
        };
        let r = Response::decode_checked(OLLAMA_RECORDED.as_bytes(), &req).unwrap();
        let a = &r.answers["intent"];
        assert_eq!(a.p_top(), Some(0.9760968387170561));
        assert_eq!(a.confidence, Some(0.9145015492062412));
        assert_eq!(a.ranked()[1].0, "refund_request");
        assert_eq!(r.usage.as_ref().map(|u| u.input_tokens), Some(111));
    }

    #[test]
    fn rejects_missing_answers_and_foreign_choices() {
        let req = intent_request();
        let missing = br#"{"model":"m","answers":{}}"#;
        assert!(matches!(
            Response::decode_checked(missing, &req),
            Err(DecisionError::Malformed(_))
        ));
        let foreign = br#"{"answers":{"intent":{"choice":"launch_rockets","probabilities":{}}}}"#;
        assert!(matches!(
            Response::decode_checked(foreign, &req),
            Err(DecisionError::Malformed(_))
        ));
        assert!(matches!(
            Response::decode_checked(b"<html>", &req),
            Err(DecisionError::Malformed(_))
        ));
    }

    #[test]
    fn p_top_is_none_for_invalid_probabilities() {
        let mut a = Answer {
            choice: Some("x".into()),
            ..Answer::default()
        };
        assert_eq!(a.p_top(), None);
        a.probabilities.insert("x".into(), f64::NAN);
        assert_eq!(a.p_top(), None);
        a.probabilities.insert("x".into(), 1.5);
        assert_eq!(a.p_top(), None);
        a.probabilities.insert("x".into(), 0.7);
        assert_eq!(a.p_top(), Some(0.7));
    }
}
