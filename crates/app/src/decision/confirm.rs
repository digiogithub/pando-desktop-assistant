//! Spoken confirmation and the low-confidence "which one?" choice (PDA-T-0102).
//!
//! Fail-closed: only an unambiguous yes executes; silence for 8 s, a no, or anything unclear
//! executes nothing. The yes/no comes from a deterministic lexical check of short answers
//! first, and from a System One `noul` question for anything longer.

use std::time::Duration;

use decide::policy::{ConfirmReason, ControlRequest};
use decide::protocol::{Answer, Question, QuestionType, Request};
use serde_json::{Map, json};

/// How long a confirmation waits for an answer.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(8);
/// Probability the `noul` question must reach to count as a "yes" (a "no" needs only a majority).
pub const YES_THRESHOLD: f64 = 0.85;

/// A yes or no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YesNo {
    /// Confirmed.
    Yes,
    /// Refused.
    No,
}

fn words(text: &str) -> Vec<String> {
    crate::voice::wer::words(text)
}

/// Deterministic reading of a short answer; `None` when it is not clearly one or the other.
#[must_use]
pub fn lexical_yes_no(text: &str) -> Option<YesNo> {
    const YES: &[&str] = &[
        "sí",
        "si",
        "vale",
        "adelante",
        "confirmo",
        "confirmado",
        "correcto",
        "claro",
        "hazlo",
        "yes",
        "ok",
        "dale",
    ];
    const NO: &[&str] = &[
        "no",
        "nada",
        "déjalo",
        "dejalo",
        "cancela",
        "olvídalo",
        "olvidalo",
        "para",
        "nope",
    ];
    let w = words(text);
    if w.is_empty() || w.len() > 4 {
        return None;
    }
    let yes = w.iter().any(|x| YES.contains(&x.as_str()));
    let no = w.iter().any(|x| NO.contains(&x.as_str()));
    match (yes, no) {
        (true, false) => Some(YesNo::Yes),
        (false, true) => Some(YesNo::No),
        _ => None,
    }
}

/// The `noul` request that asks the decision model whether `answer` confirms `prompt`.
#[must_use]
pub fn yes_no_request(
    model: &str,
    keep_alive: Option<String>,
    prompt: &str,
    answer: &str,
) -> Request {
    let question = Question {
        kind: QuestionType::Noul,
        instructions: "The assistant asked the user to confirm an action. Did the user's reply \
                       clearly confirm it? Answer yes only for an unambiguous confirmation."
            .to_owned(),
        criteria: Vec::new(),
        extra: Map::new(),
    };
    Request {
        model: model.to_owned(),
        state: json!({"asked": prompt, "reply": answer}).to_string(),
        questions: vec![("confirm".to_owned(), question)],
        keep_alive,
    }
}

/// Reads the `noul` answer fail-closed.
#[must_use]
pub fn from_answer(answer: &Answer) -> Option<YesNo> {
    let p = answer.p_top()?;
    match answer.choice.as_deref()? {
        "yes" if p >= YES_THRESHOLD => Some(YesNo::Yes),
        "no" if p >= 0.5 => Some(YesNo::No),
        _ => None,
    }
}

/// What is being asked.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    /// "¿Cancelo X?": run `request` on a yes.
    Confirm {
        /// The command to run on a yes.
        request: ControlRequest,
        /// Why it needs confirming.
        reason: ConfirmReason,
    },
    /// Low confidence: two buttons with the best interpretations.
    Pick {
        /// `(key, probability)` of the two best options.
        options: Vec<(String, f64)>,
    },
}

/// What happened to the question.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Still waiting.
    Waiting,
    /// Run this command.
    Execute(ControlRequest),
    /// Do nothing (refused, expired or unclear).
    Dismissed,
    /// Decide again with this clarified text.
    Reask(String),
}

/// An open question with its deadline (milliseconds on the controller's clock).
#[derive(Debug, Clone, PartialEq)]
pub struct Confirmation {
    /// The spoken sentence it came from.
    pub transcript: String,
    /// What is being asked.
    pub pending: Pending,
    /// When it expires.
    pub deadline_ms: u64,
}

impl Confirmation {
    /// A confirmation of `request` opened at `now_ms`.
    #[must_use]
    pub fn confirm(
        transcript: &str,
        request: ControlRequest,
        reason: ConfirmReason,
        now_ms: u64,
    ) -> Self {
        Self {
            transcript: transcript.to_owned(),
            pending: Pending::Confirm { request, reason },
            deadline_ms: now_ms + CONFIRM_TIMEOUT.as_millis() as u64,
        }
    }

    /// A two-option choice opened at `now_ms`.
    #[must_use]
    pub fn pick(transcript: &str, mut options: Vec<(String, f64)>, now_ms: u64) -> Self {
        options.truncate(2);
        Self {
            transcript: transcript.to_owned(),
            pending: Pending::Pick { options },
            deadline_ms: now_ms + CONFIRM_TIMEOUT.as_millis() as u64,
        }
    }

    /// True once the deadline passed.
    #[must_use]
    pub fn expired(&self, now_ms: u64) -> bool {
        now_ms >= self.deadline_ms
    }

    /// Applies a yes/no (spoken or clicked). Late answers do nothing.
    #[must_use]
    pub fn answer(&self, answer: Option<YesNo>, now_ms: u64) -> Verdict {
        if self.expired(now_ms) {
            return Verdict::Dismissed;
        }
        match (&self.pending, answer) {
            (Pending::Confirm { request, .. }, Some(YesNo::Yes)) => {
                Verdict::Execute(request.clone())
            }
            (Pending::Confirm { .. }, Some(YesNo::No)) => Verdict::Dismissed,
            // An unclear answer keeps waiting until the deadline: nothing runs meanwhile.
            (Pending::Confirm { .. }, None) => Verdict::Waiting,
            (Pending::Pick { .. }, _) => Verdict::Waiting,
        }
    }

    /// The user clicked option `index` of a pick.
    #[must_use]
    pub fn choose(&self, index: usize, now_ms: u64) -> Verdict {
        if self.expired(now_ms) {
            return Verdict::Dismissed;
        }
        match &self.pending {
            Pending::Pick { options } => {
                options.get(index).map_or(Verdict::Dismissed, |(key, _)| {
                    Verdict::Reask(format!("{} {}", self.transcript, key.replace('_', " ")))
                })
            }
            Pending::Confirm { .. } => Verdict::Waiting,
        }
    }
}

#[cfg(test)]
mod tests {
    use decide::policy::ControlRequest;
    use decide::questions::{Action, AgentTarget};

    use super::*;

    fn cancel() -> ControlRequest {
        ControlRequest {
            action: Action::Cancel,
            target: AgentTarget::Agent("migracion-auth".into()),
            engine: None,
            project: None,
            p: 0.97,
        }
    }

    #[test]
    fn short_answers_are_read_lexically() {
        assert_eq!(lexical_yes_no("Sí"), Some(YesNo::Yes));
        assert_eq!(lexical_yes_no("sí, cancélalo"), Some(YesNo::Yes));
        assert_eq!(lexical_yes_no("no"), Some(YesNo::No));
        assert_eq!(lexical_yes_no("sí no sé"), None);
        assert_eq!(lexical_yes_no("sí y no"), None);
        assert_eq!(
            lexical_yes_no("cuéntame más sobre el tiempo que hará mañana"),
            None
        );
        assert_eq!(lexical_yes_no(""), None);
    }

    #[test]
    fn the_noul_answer_is_fail_closed() {
        let answer = |c: &str, p: f64| Answer {
            choice: Some(c.into()),
            probabilities: [
                ("yes".to_owned(), if c == "yes" { p } else { 1.0 - p }),
                ("no".to_owned(), if c == "no" { p } else { 1.0 - p }),
            ]
            .into(),
            ..Answer::default()
        };
        assert_eq!(from_answer(&answer("yes", 0.95)), Some(YesNo::Yes));
        assert_eq!(from_answer(&answer("yes", 0.7)), None);
        assert_eq!(from_answer(&answer("no", 0.6)), Some(YesNo::No));
        assert_eq!(from_answer(&Answer::default()), None);
    }

    #[test]
    fn the_request_is_a_valid_noul_question() {
        let r = yes_no_request("tev1:0.8b", None, "¿Cancelo migracion-auth?", "sí claro");
        assert!(r.validate().is_ok(), "{:?}", r.validate());
        assert_eq!(r.questions[0].1.kind, QuestionType::Noul);
    }

    #[test]
    fn only_a_timely_yes_executes() {
        let c = Confirmation::confirm(
            "cancela migración auth",
            cancel(),
            ConfirmReason::Destructive,
            1_000,
        );
        assert_eq!(
            c.answer(Some(YesNo::Yes), 2_000),
            Verdict::Execute(cancel())
        );
        assert_eq!(c.answer(Some(YesNo::No), 2_000), Verdict::Dismissed);
        assert_eq!(c.answer(None, 2_000), Verdict::Waiting);
        // No answer within 8 s: nothing is executed, and a late yes does nothing.
        assert!(!c.expired(8_999));
        assert!(c.expired(9_000));
        assert_eq!(c.answer(Some(YesNo::Yes), 9_000), Verdict::Dismissed);
    }

    #[test]
    fn low_confidence_offers_two_buttons_that_re_ask_instead_of_executing() {
        let c = Confirmation::pick(
            "cancela el build",
            vec![
                ("build_api".into(), 0.5),
                ("build_web".into(), 0.4),
                ("x".into(), 0.1),
            ],
            0,
        );
        let Pending::Pick { options } = &c.pending else {
            panic!("pick expected");
        };
        assert_eq!(options.len(), 2);
        assert_eq!(
            c.choose(1, 100),
            Verdict::Reask("cancela el build build web".into())
        );
        assert_eq!(c.choose(5, 100), Verdict::Dismissed);
        assert_eq!(c.choose(0, 9_000), Verdict::Dismissed);
        assert_eq!(
            c.answer(Some(YesNo::Yes), 100),
            Verdict::Waiting,
            "a yes never picks"
        );
    }
}
