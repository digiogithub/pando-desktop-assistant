//! Everything that needs the user's attention (PDA-T-0079, 0080, 0081, 0083), as pure logic:
//! the queue of pending requests, the data of the permission dialog, the questions stepper, the
//! per-session auto-approve policy and the global pending counter.
//!
//! Safety (PDA-SP-0003): nothing here approves anything by itself. The ledger in
//! `pando-client` refuses a non-explicit decision for flagged requests, and the auto-approve
//! policy below returns `false` for them so the view never even tries.

use std::collections::{BTreeMap, BTreeSet};

use pando_client::ChatModel;
use pando_client::approvals::RequestBody;
use pando_client::chat::{PermissionRequest, Question, QuestionAnswer, QuestionRequest};
use pando_client::model::ItemKind;
use serde_json::Value;

/// Action name Pando uses for a sandbox escalation.
pub const ESCALATION_ACTION: &str = "execute_unsandboxed";

/// A pending request, in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// Request id.
    pub id: String,
    /// What kind it is.
    pub kind: WaitingKind,
}

/// Permission or question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingKind {
    /// A tool asks for permission.
    Permission,
    /// The agent asks the user questions.
    Question,
}

/// Pending requests of the model in transcript order (the order they arrived in).
#[must_use]
pub fn waiting_queue(model: &ChatModel) -> Vec<Waiting> {
    model
        .items()
        .iter()
        .filter_map(|item| {
            let ItemKind::Approval { request_id } = &item.kind else {
                return None;
            };
            if !model.approvals().is_pending(request_id) {
                return None;
            }
            let kind = match model.approvals().request(request_id)? {
                RequestBody::Permission(_) => WaitingKind::Permission,
                RequestBody::Question(_) => WaitingKind::Question,
            };
            Some(Waiting {
                id: request_id.clone(),
                kind,
            })
        })
        .collect()
}

/// Scope of a "allow for the session" grant, from `grant_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantScope {
    /// No key.
    None,
    /// `prefix:<command prefix>`.
    Prefix(String),
    /// `exact:<command>`.
    Exact(String),
    /// Some other key, shown verbatim.
    Other(String),
}

/// Parses a `grant_key`.
#[must_use]
pub fn grant_scope(key: Option<&str>) -> GrantScope {
    match key.map(str::trim).filter(|k| !k.is_empty()) {
        None => GrantScope::None,
        Some(k) => {
            if let Some(rest) = k.strip_prefix("prefix:") {
                GrantScope::Prefix(rest.to_owned())
            } else if let Some(rest) = k.strip_prefix("exact:") {
                GrantScope::Exact(rest.to_owned())
            } else {
                GrantScope::Other(k.to_owned())
            }
        }
    }
}

/// What the permission dialog shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionView {
    /// Request id.
    pub id: String,
    /// Tool asking.
    pub tool: String,
    /// Description.
    pub description: String,
    /// Path concerned.
    pub path: String,
    /// Command line for executions.
    pub command: Option<String>,
    /// Parameters, pretty-printed.
    pub params: String,
    /// Sandbox escalation variant.
    pub escalation: bool,
    /// Model's justification.
    pub justification: Option<String>,
    /// Scope of the session grant.
    pub scope: GrantScope,
    /// Only a deliberate click may approve it.
    pub explicit_only: bool,
    /// "Permitir en sesión" is offered.
    pub can_session: bool,
}

/// Builds the dialog data of a permission request.
#[must_use]
pub fn permission_view(p: &PermissionRequest, flagged: bool) -> PermissionView {
    let escalation = p.action == ESCALATION_ACTION;
    let command = p
        .params
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_owned);
    PermissionView {
        id: p.id.clone(),
        tool: p.tool_name.clone(),
        description: p.description.clone(),
        path: p.path.clone(),
        command,
        params: if p.params.is_null() {
            String::new()
        } else {
            serde_json::to_string_pretty(&p.params).unwrap_or_default()
        },
        escalation,
        justification: p.justification.clone().filter(|j| !j.trim().is_empty()),
        scope: grant_scope(p.grant_key.as_deref()),
        explicit_only: flagged || escalation,
        can_session: !flagged,
    }
}

/// Whether the per-session auto-approve policy may approve `p` on its own. Flagged requests
/// (`require_explicit_approval`, `never_auto_approve`) and sandbox escalations never qualify.
#[must_use]
pub fn auto_approvable(p: &PermissionRequest) -> bool {
    !(p.require_explicit_approval || p.never_auto_approve || p.action == ESCALATION_ACTION)
}

// ------------------------------------------------------------------ questions stepper

/// The user's draft answer to one question.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnswerDraft {
    /// Selected option indices.
    pub selected: BTreeSet<usize>,
    /// "Otro" is chosen.
    pub other_on: bool,
    /// Free text of "Otro".
    pub other_text: String,
}

/// The stepper over the questions of one request, then a review step.
#[derive(Debug, Clone)]
pub struct QuestionFlow {
    request_id: String,
    session_id: String,
    questions: Vec<Question>,
    drafts: Vec<AnswerDraft>,
    step: usize,
}

impl QuestionFlow {
    /// A flow over `req`.
    #[must_use]
    pub fn new(req: &QuestionRequest) -> Self {
        Self {
            request_id: req.id.clone(),
            session_id: req.session_id.clone(),
            questions: req.questions.clone(),
            drafts: vec![AnswerDraft::default(); req.questions.len()],
            step: 0,
        }
    }

    /// Request id.
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Session id.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Current step (`questions().len()` is the review).
    #[must_use]
    pub fn step(&self) -> usize {
        self.step
    }

    /// The questions.
    #[must_use]
    pub fn questions(&self) -> &[Question] {
        &self.questions
    }

    /// Drafts, one per question.
    #[must_use]
    pub fn drafts(&self) -> &[AnswerDraft] {
        &self.drafts
    }

    /// There is a review step (more than one question).
    #[must_use]
    pub fn has_review(&self) -> bool {
        self.questions.len() > 1
    }

    /// On the review step.
    #[must_use]
    pub fn is_review(&self) -> bool {
        self.has_review() && self.step >= self.questions.len()
    }

    /// The current step is the last one: its button sends.
    #[must_use]
    pub fn is_last(&self) -> bool {
        if self.has_review() {
            self.is_review()
        } else {
            true
        }
    }

    /// Index of the question being answered.
    #[must_use]
    pub fn current(&self) -> Option<usize> {
        (self.step < self.questions.len()).then_some(self.step)
    }

    /// Toggles option `opt` of question `q` (single-select replaces the choice).
    pub fn toggle(&mut self, q: usize, opt: usize) {
        let multi = self.questions.get(q).is_some_and(|x| x.multi_select);
        let Some(d) = self.drafts.get_mut(q) else {
            return;
        };
        if multi {
            if !d.selected.remove(&opt) {
                d.selected.insert(opt);
            }
        } else {
            d.selected.clear();
            d.selected.insert(opt);
            d.other_on = false;
        }
    }

    /// Chooses or clears "Otro" of question `q`.
    pub fn set_other(&mut self, q: usize, on: bool) {
        let multi = self.questions.get(q).is_some_and(|x| x.multi_select);
        if let Some(d) = self.drafts.get_mut(q) {
            d.other_on = on;
            if on && !multi {
                d.selected.clear();
            }
        }
    }

    /// Sets the free text of "Otro" (choosing it when text is typed).
    pub fn set_other_text(&mut self, q: usize, text: &str) {
        if let Some(d) = self.drafts.get_mut(q) {
            text.clone_into(&mut d.other_text);
        }
    }

    /// Question `q` has an answer.
    #[must_use]
    pub fn is_answered(&self, q: usize) -> bool {
        self.drafts.get(q).is_some_and(|d| {
            !d.selected.is_empty() || (d.other_on && !d.other_text.trim().is_empty())
        })
    }

    /// The current step may advance.
    #[must_use]
    pub fn can_next(&self) -> bool {
        match self.current() {
            Some(q) => self.is_answered(q),
            None => (0..self.questions.len()).all(|q| self.is_answered(q)),
        }
    }

    /// Advances (to the next question, the review or, on the last step, nothing).
    pub fn next(&mut self) {
        if self.can_next() && !self.is_last() {
            self.step += 1;
        }
    }

    /// Goes back one step.
    pub fn back(&mut self) {
        self.step = self.step.saturating_sub(1);
    }

    /// Whether Send is available now.
    #[must_use]
    pub fn can_send(&self) -> bool {
        self.is_last() && (0..self.questions.len()).all(|q| self.is_answered(q))
    }

    /// The answers for the wire, `None` while incomplete.
    #[must_use]
    pub fn answers(&self) -> Option<Vec<QuestionAnswer>> {
        if !(0..self.questions.len()).all(|q| self.is_answered(q)) {
            return None;
        }
        Some(
            self.questions
                .iter()
                .zip(&self.drafts)
                .map(|(q, d)| QuestionAnswer {
                    question_id: q.id.clone(),
                    selected: d
                        .selected
                        .iter()
                        .filter_map(|i| q.options.get(*i))
                        .map(|o| o.label.clone())
                        .collect(),
                    other_text: if d.other_on {
                        d.other_text.trim().to_owned()
                    } else {
                        String::new()
                    },
                })
                .collect(),
        )
    }
}

// ------------------------------------------------------------------ global pending counter

/// Pending items per session, aggregated from `/pending` polls and the open session.
#[derive(Debug, Clone, Default)]
pub struct PendingTally {
    by_session: BTreeMap<String, usize>,
}

impl PendingTally {
    /// Sets the count of one session (0 removes it).
    pub fn set(&mut self, session_id: &str, count: usize) {
        if count == 0 {
            self.by_session.remove(session_id);
        } else {
            self.by_session.insert(session_id.to_owned(), count);
        }
    }

    /// Forgets sessions that are not in `keep` (deleted or no longer polled).
    pub fn retain(&mut self, keep: &BTreeSet<String>) {
        self.by_session.retain(|id, _| keep.contains(id));
    }

    /// Total pending items.
    #[must_use]
    pub fn total(&self) -> usize {
        self.by_session.values().sum()
    }

    /// Sessions waiting for the user.
    #[must_use]
    pub fn sessions(&self) -> usize {
        self.by_session.len()
    }

    /// The session to jump to: `prefer` when it waits (so a click stays put), else the first by
    /// id order.
    #[must_use]
    pub fn first(&self, current: Option<&str>) -> Option<&str> {
        match current {
            Some(c) if self.by_session.contains_key(c) => None,
            _ => self.by_session.keys().next().map(String::as_str),
        }
    }

    /// Count of one session.
    #[must_use]
    pub fn count(&self, session_id: &str) -> usize {
        self.by_session.get(session_id).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pando_client::approvals::{Decision, Origin, Refused};
    use pando_client::chat::{ChatEvent, QuestionOption};

    fn perm(id: &str, flagged: bool) -> PermissionRequest {
        PermissionRequest {
            id: id.into(),
            session_id: "s".into(),
            tool_name: "bash".into(),
            require_explicit_approval: flagged,
            ..PermissionRequest::default()
        }
    }

    fn question(id: &str, multi: bool) -> Question {
        Question {
            id: id.into(),
            header: "H".into(),
            question: "Q?".into(),
            multi_select: multi,
            options: ["a", "b", "c"]
                .iter()
                .map(|l| QuestionOption {
                    label: (*l).into(),
                    description: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn queue_keeps_arrival_order_and_drops_resolved() {
        let mut m = ChatModel::default();
        m.apply(&ChatEvent::PermissionRequest(perm("p1", false)));
        m.apply(&ChatEvent::QuestionRequest(QuestionRequest {
            id: "q1".into(),
            session_id: "s".into(),
            questions: vec![question("x", false)],
        }));
        m.apply(&ChatEvent::PermissionRequest(perm("p2", true)));
        let q = waiting_queue(&m);
        assert_eq!(
            q.iter()
                .map(|w| (w.id.as_str(), w.kind))
                .collect::<Vec<_>>(),
            [
                ("p1", WaitingKind::Permission),
                ("q1", WaitingKind::Question),
                ("p2", WaitingKind::Permission)
            ]
        );
        m.decide("p1", Decision::Deny, Origin::Explicit)
            .expect("deny");
        assert_eq!(waiting_queue(&m).len(), 2);
    }

    #[test]
    fn flagged_requests_are_never_auto_approvable() {
        assert!(auto_approvable(&perm("a", false)));
        assert!(!auto_approvable(&perm("a", true)));
        let mut never = perm("n", false);
        never.never_auto_approve = true;
        assert!(!auto_approvable(&never));
        let mut esc = perm("e", false);
        esc.action = ESCALATION_ACTION.into();
        assert!(!auto_approvable(&esc));
    }

    #[test]
    fn the_ledger_still_refuses_auto_and_voice_for_flagged_requests() {
        // The view trusts this: even a buggy auto-approve call cannot approve a flagged item.
        let mut m = ChatModel::default();
        m.apply(&ChatEvent::PermissionRequest(perm("f", true)));
        assert_eq!(
            m.decide("f", Decision::Approve, Origin::Auto).err(),
            Some(Refused::NeedsExplicit)
        );
        assert_eq!(
            m.decide("f", Decision::Approve, Origin::Voice).err(),
            Some(Refused::NeedsExplicit)
        );
        assert_eq!(
            m.decide("f", Decision::ApproveSession, Origin::Explicit)
                .err(),
            Some(Refused::SessionGrantNotAllowed)
        );
        assert!(m.approvals().is_pending("f"));
        assert!(m.decide("f", Decision::Approve, Origin::Explicit).is_ok());
    }

    #[test]
    fn escalation_view_exposes_command_justification_and_scope() {
        let mut p = perm("e", false);
        p.action = ESCALATION_ACTION.into();
        p.params = serde_json::json!({"command": "npm install"});
        p.justification = Some("needs network".into());
        p.grant_key = Some("prefix:npm install".into());
        let v = permission_view(&p, false);
        assert!(v.escalation && v.explicit_only);
        assert_eq!(v.command.as_deref(), Some("npm install"));
        assert_eq!(v.scope, GrantScope::Prefix("npm install".into()));
        assert!(v.can_session);
        let flagged = permission_view(&perm("f", true), true);
        assert!(flagged.explicit_only && !flagged.can_session);
        assert_eq!(
            grant_scope(Some("exact:ls")),
            GrantScope::Exact("ls".into())
        );
        assert_eq!(grant_scope(Some("  ")), GrantScope::None);
        assert_eq!(grant_scope(Some("tool")), GrantScope::Other("tool".into()));
    }

    fn request(qs: Vec<Question>) -> QuestionRequest {
        QuestionRequest {
            id: "r".into(),
            session_id: "s".into(),
            questions: qs,
        }
    }

    #[test]
    fn single_question_sends_without_a_review_step() {
        let mut f = QuestionFlow::new(&request(vec![question("a", false)]));
        assert!(!f.has_review() && f.is_last() && !f.can_send());
        f.toggle(0, 1);
        f.toggle(0, 2); // single select replaces
        assert!(f.can_send());
        let a = f.answers().expect("answers");
        assert_eq!(a[0].selected, ["c"]);
    }

    #[test]
    fn stepper_requires_an_answer_then_reviews() {
        let mut f = QuestionFlow::new(&request(vec![question("a", false), question("b", true)]));
        f.next();
        assert_eq!(f.step(), 0, "cannot skip an unanswered question");
        f.toggle(0, 0);
        f.next();
        assert_eq!(f.step(), 1);
        f.toggle(1, 0);
        f.toggle(1, 2);
        f.toggle(1, 0); // multi-select toggles off
        f.next();
        assert!(f.is_review() && f.can_send());
        f.back();
        assert_eq!(f.step(), 1);
        f.next();
        let a = f.answers().expect("answers");
        assert_eq!(
            (a[0].selected.clone(), a[1].selected.clone()),
            (vec!["a".to_owned()], vec!["c".to_owned()])
        );
    }

    #[test]
    fn other_needs_text_and_excludes_options_in_single_select() {
        let mut f = QuestionFlow::new(&request(vec![question("a", false)]));
        f.toggle(0, 0);
        f.set_other(0, true);
        assert!(!f.is_answered(0), "no text yet and the option was cleared");
        f.set_other_text(0, "  my own  ");
        assert!(f.is_answered(0));
        let a = f.answers().expect("answers");
        assert!(a[0].selected.is_empty());
        assert_eq!(a[0].other_text, "my own");
        f.toggle(0, 1);
        assert!(!f.drafts()[0].other_on);
    }

    #[test]
    fn tally_counts_unselected_sessions_and_jumps_to_them() {
        let mut t = PendingTally::default();
        t.set("a", 2);
        t.set("b", 1);
        assert_eq!((t.total(), t.sessions()), (3, 2));
        assert_eq!(t.first(Some("c")), Some("a"));
        assert_eq!(t.first(Some("a")), None, "already there");
        t.set("a", 0);
        assert_eq!(t.first(None), Some("b"));
        t.retain(&BTreeSet::new());
        assert_eq!(t.total(), 0);
    }
}
