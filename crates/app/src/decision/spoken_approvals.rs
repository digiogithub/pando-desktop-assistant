//! Approving, denying and answering questions by voice (PDA-T-0103).
//!
//! The assistant reads the pending item aloud; the user answers "aprueba", "deniega", "la
//! segunda opción" or "otro: ...". Every answer goes through the fail-closed
//! `pando_client::approvals::Approvals` ledger with `Origin::Voice`: the ledger itself refuses a
//! flagged request (`require_explicit_approval` / `never_auto_approve`), and this module
//! additionally keeps sandbox escalations and "allow for the session" on screen. A spoken
//! "sí" can therefore never approve something that needs a click.

use std::time::Duration;

use pando_client::approvals::{Approvals, Decision, Origin, Refused, RequestBody, Resolution};
use pando_client::chat::{PermissionRequest, QuestionAnswer, QuestionRequest};
use rust_i18n::t;

use crate::voice::wer::words;

/// A spoken answer, interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spoken {
    /// "aprueba", "permítelo".
    Approve,
    /// "deniega", "recházalo".
    Deny,
    /// "la segunda opción" (zero-based index).
    Option(usize),
    /// "otro: ..." free text.
    Other(String),
    /// Nothing recognisable.
    Unclear,
}

/// True when a permission may not be granted by voice at all.
#[must_use]
pub fn needs_screen(req: &PermissionRequest) -> bool {
    req.require_explicit_approval || req.never_auto_approve || req.action == "execute_unsandboxed"
}

/// Interprets what the user said about a pending request. `n_options` bounds ordinal answers.
#[must_use]
pub fn interpret(text: &str, n_options: usize) -> Spoken {
    let lower = text.trim().to_lowercase();
    for prefix in ["otro:", "otra:", "otro ", "otra "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let original = text.trim()[prefix.len()..].trim();
            if !rest.trim().is_empty() && !original.is_empty() {
                return Spoken::Other(original.to_owned());
            }
        }
    }
    let w = words(text);
    let has = |list: &[&str]| w.iter().any(|x| list.contains(&x.as_str()));
    const ORD: [(&[&str], usize); 5] = [
        (&["primera", "primero", "uno", "1"], 0),
        (&["segunda", "segundo", "dos", "2"], 1),
        (&["tercera", "tercero", "tres", "3"], 2),
        (&["cuarta", "cuarto", "cuatro", "4"], 3),
        (&["quinta", "quinto", "cinco", "5"], 4),
    ];
    if has(&[
        "opción", "opcion", "primera", "segunda", "tercera", "cuarta", "quinta", "primero",
        "segundo", "tercero",
    ]) {
        for (names, idx) in ORD {
            if has(names) && idx < n_options {
                return Spoken::Option(idx);
            }
        }
    }
    let approve = has(&[
        "aprueba",
        "aprobar",
        "aprobado",
        "permite",
        "permítelo",
        "permitelo",
        "autoriza",
        "acepta",
        "sí",
        "si",
        "vale",
        "adelante",
    ]);
    let deny = has(&[
        "deniega",
        "denegar",
        "rechaza",
        "recházalo",
        "rechazalo",
        "no",
        "niega",
        "deniégalo",
        "denielo",
    ]);
    match (approve, deny) {
        (true, false) => Spoken::Approve,
        (false, true) => Spoken::Deny,
        _ => Spoken::Unclear,
    }
}

/// What to read aloud for a pending request.
#[must_use]
pub fn read_aloud(body: &RequestBody) -> String {
    match body {
        RequestBody::Permission(p) => {
            let what = if p.description.trim().is_empty() {
                p.tool_name.clone()
            } else {
                p.description.trim().to_owned()
            };
            if needs_screen(p) {
                t!("voice.approvals.permission_screen", what = what).to_string()
            } else {
                t!("voice.approvals.permission", what = what).to_string()
            }
        }
        RequestBody::Question(q) => match q.questions.first() {
            Some(first) => {
                let mut text = first.question.trim().to_owned();
                for (i, o) in first.options.iter().enumerate() {
                    text.push_str(&format!(" {}: {}.", i + 1, o.label));
                }
                text
            }
            None => t!("voice.approvals.question_empty").to_string(),
        },
    }
}

/// How an answer ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    /// The ledger resolved it.
    Resolved(Resolution),
    /// A click or key press on screen is required.
    NeedsScreen,
    /// Did not understand / nothing to apply.
    Unclear,
    /// The request is unknown or already resolved.
    Gone,
}

fn first_question(q: &QuestionRequest) -> Option<&pando_client::chat::Question> {
    q.questions.first()
}

/// Applies a spoken answer to request `id`. Never approves anything the ledger flags.
pub fn apply(ledger: &mut Approvals, id: &str, spoken: &Spoken) -> Applied {
    let Some(body) = ledger.request(id).cloned() else {
        return Applied::Gone;
    };
    let decision = match (&body, spoken) {
        (RequestBody::Permission(p), Spoken::Approve) => {
            if needs_screen(p) {
                return Applied::NeedsScreen;
            }
            Decision::Approve
        }
        (RequestBody::Permission(_), Spoken::Deny) => Decision::Deny,
        (RequestBody::Question(_), Spoken::Deny) => Decision::Deny,
        (RequestBody::Question(q), Spoken::Option(i)) => {
            let Some(first) = first_question(q) else {
                return Applied::Unclear;
            };
            let Some(opt) = first.options.get(*i) else {
                return Applied::Unclear;
            };
            Decision::Answer(vec![QuestionAnswer {
                question_id: first.id.clone(),
                selected: vec![opt.label.clone()],
                other_text: String::new(),
            }])
        }
        (RequestBody::Question(q), Spoken::Other(text)) => {
            let Some(first) = first_question(q) else {
                return Applied::Unclear;
            };
            Decision::Answer(vec![QuestionAnswer {
                question_id: first.id.clone(),
                selected: Vec::new(),
                other_text: text.clone(),
            }])
        }
        _ => return Applied::Unclear,
    };
    match ledger.decide(id, decision, Origin::Voice) {
        Ok(r) => Applied::Resolved(r),
        Err(Refused::NeedsExplicit | Refused::SessionGrantNotAllowed) => Applied::NeedsScreen,
        Err(Refused::Unknown | Refused::AlreadyResolved) => Applied::Gone,
        Err(Refused::WrongKind) => Applied::Unclear,
    }
}

/// A fresh ledger with the usual 120 s timeout (for tests and the voice flow).
#[must_use]
pub fn ledger() -> Approvals {
    Approvals::new(Duration::from_secs(120))
}

#[cfg(test)]
mod tests {
    use pando_client::chat::{Question, QuestionOption};

    use super::*;

    fn perm(id: &str, flagged: bool, action: &str) -> PermissionRequest {
        PermissionRequest {
            id: id.into(),
            session_id: "s".into(),
            tool_name: "bash".into(),
            description: "ejecutar cargo test".into(),
            action: action.into(),
            require_explicit_approval: flagged,
            ..PermissionRequest::default()
        }
    }

    fn question() -> QuestionRequest {
        QuestionRequest {
            id: "q".into(),
            session_id: "s".into(),
            questions: vec![Question {
                id: "q1".into(),
                question: "¿Qué rama?".into(),
                options: vec![
                    QuestionOption {
                        label: "main".into(),
                        ..QuestionOption::default()
                    },
                    QuestionOption {
                        label: "dev".into(),
                        ..QuestionOption::default()
                    },
                ],
                ..Question::default()
            }],
        }
    }

    #[test]
    fn interprets_the_phrases() {
        assert_eq!(interpret("aprueba", 0), Spoken::Approve);
        assert_eq!(interpret("sí, adelante", 0), Spoken::Approve);
        assert_eq!(interpret("deniega eso", 0), Spoken::Deny);
        assert_eq!(interpret("no", 0), Spoken::Deny);
        assert_eq!(interpret("la segunda opción", 2), Spoken::Option(1));
        assert_eq!(
            interpret("la tercera opción", 2),
            Spoken::Unclear,
            "out of range"
        );
        assert_eq!(
            interpret("Otro: la rama release", 2),
            Spoken::Other("la rama release".into())
        );
        assert_eq!(interpret("no sé, sí o no", 0), Spoken::Unclear);
        assert_eq!(interpret("", 0), Spoken::Unclear);
    }

    #[test]
    fn a_plain_permission_can_be_approved_by_voice() {
        let mut l = ledger();
        l.register_permission(perm("p1", false, "execute"), Duration::ZERO);
        assert_eq!(
            apply(&mut l, "p1", &Spoken::Approve),
            Applied::Resolved(Resolution::Approved)
        );
        assert_eq!(
            apply(&mut l, "p1", &Spoken::Approve),
            Applied::Gone,
            "resolutions are final"
        );
    }

    #[test]
    fn a_flagged_permission_is_never_approved_by_voice() {
        let mut l = ledger();
        l.register_permission(perm("p1", true, "execute"), Duration::ZERO);
        assert_eq!(apply(&mut l, "p1", &Spoken::Approve), Applied::NeedsScreen);
        assert!(l.is_pending("p1"), "nothing changed");
        // Denying is always safe.
        assert!(matches!(
            apply(&mut l, "p1", &Spoken::Deny),
            Applied::Resolved(Resolution::Denied(_))
        ));
    }

    #[test]
    fn sandbox_escalations_stay_on_screen() {
        let mut l = ledger();
        l.register_permission(perm("p2", false, "execute_unsandboxed"), Duration::ZERO);
        assert_eq!(apply(&mut l, "p2", &Spoken::Approve), Applied::NeedsScreen);
        assert!(l.is_pending("p2"));
    }

    #[test]
    fn questions_are_answered_by_option_or_free_text() {
        let mut l = ledger();
        l.register_question(question(), Duration::ZERO);
        match apply(&mut l, "q", &Spoken::Option(1)) {
            Applied::Resolved(Resolution::Answered(a)) => {
                assert_eq!(a[0].selected, vec!["dev".to_owned()])
            }
            other => panic!("{other:?}"),
        }
        let mut l = ledger();
        l.register_question(question(), Duration::ZERO);
        match apply(&mut l, "q", &Spoken::Other("release".into())) {
            Applied::Resolved(Resolution::Answered(a)) => assert_eq!(a[0].other_text, "release"),
            other => panic!("{other:?}"),
        }
        let mut l = ledger();
        l.register_question(question(), Duration::ZERO);
        assert_eq!(
            apply(&mut l, "q", &Spoken::Approve),
            Applied::Unclear,
            "approving a question makes no sense"
        );
        assert_eq!(apply(&mut l, "zzz", &Spoken::Deny), Applied::Gone);
    }

    #[test]
    fn reading_aloud_warns_when_the_screen_is_needed() {
        crate::i18n::apply(Some("es"));
        let a = read_aloud(&RequestBody::Permission(perm("p", false, "execute")));
        let b = read_aloud(&RequestBody::Permission(perm("p", true, "execute")));
        assert!(a.contains("ejecutar cargo test"));
        assert!(b.to_lowercase().contains("pantalla"), "{b}");
        assert!(read_aloud(&RequestBody::Question(question())).contains("1: main"));
    }
}
