//! Fail-closed approvals ledger (PDA-T-0082, safety).
//!
//! Every interrupt the user must answer (a Pando permission request, an `AskUserQuestion`) is
//! registered here. The **only** way to approve is an explicit approving [`Decision`] for that
//! exact id while it is still pending, coming from an allowed [`Origin`]. Anything else resolves
//! it as denied (a question: cancelled): the deadline passing, the panel closing, a session
//! switch, quit, a cancelled run, or Pando having given up. A resolution is final, so a late
//! "Approve" after a timeout cannot do anything.
//!
//! Requests flagged `require_explicit_approval` or `never_auto_approve` can only be approved
//! with [`Origin::Explicit`] (a deliberate click or key), never by voice or by an automatic
//! shortcut, and never with "allow for the session" ([`Decision::ApproveSession`]), which would
//! pre-approve future requests.
//!
//! Resolutions are queued in an outbox as wire answers ([`Outgoing`]); the driver sends them
//! with [`crate::PandoClient::permissions_respond`] / `questions_respond` and calls
//! [`Approvals::mark_delivered`]. Time comes from a [`Clock`] so tests never sleep.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::chat::{PermissionAction, PermissionRequest, QuestionAnswer, QuestionRequest};

/// Default time a request waits for an answer.
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

/// A monotonic clock (time since an arbitrary origin).
pub trait Clock: Send + Sync {
    /// Time elapsed since the clock's origin.
    fn now(&self) -> Duration;
}

/// The real clock.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: std::time::Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// What kind of answer a request takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalKind {
    /// A permission request (tool use).
    Permission,
    /// An `AskUserQuestion`.
    Question,
}

/// An answer from the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Allow this one request.
    Approve,
    /// Allow it and similar ones for the session (refused for flagged requests).
    ApproveSession,
    /// Refuse.
    Deny,
    /// Answer a question.
    Answer(Vec<QuestionAnswer>),
}

/// Where a decision came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A deliberate click or key press on the request itself.
    Explicit,
    /// A spoken confirmation.
    Voice,
    /// A policy or remembered shortcut.
    Auto,
}

/// Why something was denied without the user pressing Deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// The user pressed Deny.
    User,
    /// The deadline passed.
    Timeout,
    /// The chat panel was closed.
    PanelClosed,
    /// The user switched to another session.
    SessionSwitch,
    /// The app is quitting.
    Quit,
    /// The run was cancelled.
    Cancelled,
    /// Pando resolved it first, or the run is gone.
    ServerExpired,
}

impl DenyReason {
    /// Stable name for logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user_denied",
            Self::Timeout => "timeout",
            Self::PanelClosed => "panel_closed",
            Self::SessionSwitch => "session_switch",
            Self::Quit => "quit",
            Self::Cancelled => "cancelled",
            Self::ServerExpired => "server_expired",
        }
    }
}

/// The final state of one request.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// Approved once.
    Approved,
    /// Approved for the session.
    ApprovedForSession,
    /// Denied.
    Denied(DenyReason),
    /// A question was answered.
    Answered(Vec<QuestionAnswer>),
    /// A question was cancelled.
    Cancelled(DenyReason),
}

impl Resolution {
    /// Only an explicit approval allows the action.
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved | Self::ApprovedForSession)
    }
}

/// Why a decision was refused (nothing changed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Unknown id.
    Unknown,
    /// Already resolved: resolutions are final.
    AlreadyResolved,
    /// The decision does not fit the request (approving a question, answering a permission).
    WrongKind,
    /// The request is flagged and the decision did not come from [`Origin::Explicit`].
    NeedsExplicit,
    /// "Allow for the session" is not available for flagged requests.
    SessionGrantNotAllowed,
}

/// The wire answer to send for a resolved request.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    /// `POST /permissions/respond`.
    Permission {
        /// Request id.
        id: String,
        /// Session.
        session_id: String,
        /// `Allow|AllowSession|Deny`; anything not explicitly approved is `Deny`.
        action: PermissionAction,
    },
    /// `POST /questions/respond`.
    Question {
        /// Request id.
        id: String,
        /// Session.
        session_id: String,
        /// Answers (empty when cancelled).
        answers: Vec<QuestionAnswer>,
        /// The question was cancelled.
        cancelled: bool,
    },
}

/// What a card shows.
#[derive(Debug, Clone, PartialEq)]
pub enum RequestBody {
    /// A permission request.
    Permission(PermissionRequest),
    /// A question.
    Question(QuestionRequest),
}

#[derive(Debug)]
struct Entry {
    body: RequestBody,
    kind: ApprovalKind,
    flagged: bool,
    deadline: Duration,
    resolution: Option<Resolution>,
    delivered: bool,
}

impl Entry {
    fn session_id(&self) -> &str {
        match &self.body {
            RequestBody::Permission(p) => &p.session_id,
            RequestBody::Question(q) => &q.session_id,
        }
    }
}

/// The approvals of one session.
#[derive(Debug)]
pub struct Approvals {
    timeout: Duration,
    entries: BTreeMap<String, Entry>,
}

impl Default for Approvals {
    fn default() -> Self {
        Self::new(DEFAULT_APPROVAL_TIMEOUT)
    }
}

impl Approvals {
    /// An empty ledger; each request waits `timeout` for its answer.
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            entries: BTreeMap::new(),
        }
    }

    /// Registers a permission request. Re-registering an id (a replay) keeps the first entry.
    pub fn register_permission(&mut self, req: PermissionRequest, now: Duration) {
        let flagged = req.require_explicit_approval || req.never_auto_approve;
        self.insert(
            req.id.clone(),
            RequestBody::Permission(req),
            ApprovalKind::Permission,
            flagged,
            now,
        );
    }

    /// Registers a question. Questions are never "approvals", but they still fail closed.
    pub fn register_question(&mut self, req: QuestionRequest, now: Duration) {
        self.insert(
            req.id.clone(),
            RequestBody::Question(req),
            ApprovalKind::Question,
            false,
            now,
        );
    }

    fn insert(
        &mut self,
        id: String,
        body: RequestBody,
        kind: ApprovalKind,
        flagged: bool,
        now: Duration,
    ) {
        let deadline = now.saturating_add(self.timeout);
        self.entries.entry(id).or_insert(Entry {
            body,
            kind,
            flagged,
            deadline,
            resolution: None,
            delivered: false,
        });
    }

    /// Applies a user decision. On `Err` nothing changes.
    pub fn decide(
        &mut self,
        id: &str,
        decision: Decision,
        origin: Origin,
    ) -> Result<Resolution, Refused> {
        let e = self.entries.get_mut(id).ok_or(Refused::Unknown)?;
        if e.resolution.is_some() {
            return Err(Refused::AlreadyResolved);
        }
        let resolution = match (e.kind, decision) {
            (ApprovalKind::Permission, Decision::Approve) => {
                if e.flagged && origin != Origin::Explicit {
                    return Err(Refused::NeedsExplicit);
                }
                Resolution::Approved
            }
            (ApprovalKind::Permission, Decision::ApproveSession) => {
                if e.flagged {
                    return Err(Refused::SessionGrantNotAllowed);
                }
                if origin != Origin::Explicit {
                    return Err(Refused::NeedsExplicit);
                }
                Resolution::ApprovedForSession
            }
            (ApprovalKind::Permission, Decision::Deny) => Resolution::Denied(DenyReason::User),
            (ApprovalKind::Question, Decision::Answer(a)) => Resolution::Answered(a),
            (ApprovalKind::Question, Decision::Deny) => Resolution::Cancelled(DenyReason::User),
            _ => return Err(Refused::WrongKind),
        };
        e.resolution = Some(resolution.clone());
        Ok(resolution)
    }

    /// Resolves one pending request as denied for `reason`. `false` when not pending.
    pub fn deny(&mut self, id: &str, reason: DenyReason) -> bool {
        match self.entries.get_mut(id) {
            Some(e) if e.resolution.is_none() => {
                e.resolution = Some(denial(e.kind, reason));
                true
            }
            _ => false,
        }
    }

    /// Resolves every pending request whose deadline passed as [`DenyReason::Timeout`].
    pub fn expire(&mut self, now: Duration) -> Vec<String> {
        let mut out = Vec::new();
        for (id, e) in &mut self.entries {
            if e.resolution.is_none() && now >= e.deadline {
                e.resolution = Some(denial(e.kind, DenyReason::Timeout));
                out.push(id.clone());
            }
        }
        out
    }

    /// Resolves every pending request as denied for `reason` (panel close, session switch,
    /// quit, cancelled run, server gone). Returns the ids it resolved.
    pub fn deny_all(&mut self, reason: DenyReason) -> Vec<String> {
        let mut out = Vec::new();
        for (id, e) in &mut self.entries {
            if e.resolution.is_none() {
                e.resolution = Some(denial(e.kind, reason));
                out.push(id.clone());
            }
        }
        out
    }

    /// The resolution of `id`, once there is one.
    pub fn resolution(&self, id: &str) -> Option<&Resolution> {
        self.entries.get(id)?.resolution.as_ref()
    }

    /// The request behind `id`.
    pub fn request(&self, id: &str) -> Option<&RequestBody> {
        self.entries.get(id).map(|e| &e.body)
    }

    /// Whether `id` is registered and still waiting.
    pub fn is_pending(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|e| e.resolution.is_none())
    }

    /// Whether `id` is flagged (`require_explicit_approval` / `never_auto_approve`).
    pub fn is_flagged(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|e| e.flagged)
    }

    /// Ids still waiting, in id order.
    pub fn pending_ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.resolution.is_none())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The earliest deadline among pending requests (when to call [`Approvals::expire`]).
    pub fn next_deadline(&self) -> Option<Duration> {
        self.entries
            .values()
            .filter(|e| e.resolution.is_none())
            .map(|e| e.deadline)
            .min()
    }

    /// Resolved answers not yet delivered to Pando. Anything but an explicit approval is a
    /// `Deny` / cancelled question on the wire.
    pub fn outbox(&self) -> Vec<Outgoing> {
        self.entries
            .iter()
            .filter(|(_, e)| !e.delivered)
            .filter_map(|(id, e)| {
                let r = e.resolution.as_ref()?;
                Some(match r {
                    Resolution::Approved => perm(id, e, PermissionAction::Allow),
                    Resolution::ApprovedForSession => perm(id, e, PermissionAction::AllowSession),
                    Resolution::Denied(_) => perm(id, e, PermissionAction::Deny),
                    Resolution::Answered(a) => Outgoing::Question {
                        id: id.clone(),
                        session_id: e.session_id().to_owned(),
                        answers: a.clone(),
                        cancelled: false,
                    },
                    Resolution::Cancelled(_) => Outgoing::Question {
                        id: id.clone(),
                        session_id: e.session_id().to_owned(),
                        answers: Vec::new(),
                        cancelled: true,
                    },
                })
            })
            .collect()
    }

    /// Sends every undelivered resolution to Pando and marks the ones that got through.
    /// Returns how many were delivered. Failures stay in the outbox for the next call.
    pub async fn deliver(&mut self, client: &crate::PandoClient) -> usize {
        let mut sent = 0;
        for o in self.outbox() {
            let (id, res) = match &o {
                Outgoing::Permission {
                    id,
                    session_id,
                    action,
                } => (
                    id.clone(),
                    client
                        .permissions_respond(id, session_id, *action)
                        .await
                        .map(|_| ()),
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
                        .map(|_| ()),
                ),
            };
            if res.is_ok() {
                self.mark_delivered(&id);
                sent += 1;
            }
        }
        sent
    }

    /// Records that the answer for `id` reached Pando.
    pub fn mark_delivered(&mut self, id: &str) {
        if let Some(e) = self.entries.get_mut(id) {
            e.delivered = true;
        }
    }
}

fn perm(id: &str, e: &Entry, action: PermissionAction) -> Outgoing {
    Outgoing::Permission {
        id: id.to_owned(),
        session_id: e.session_id().to_owned(),
        action,
    }
}

fn denial(kind: ApprovalKind, reason: DenyReason) -> Resolution {
    match kind {
        ApprovalKind::Permission => Resolution::Denied(reason),
        ApprovalKind::Question => Resolution::Cancelled(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Duration = Duration::from_secs(10);

    fn perm_req(id: &str, explicit: bool, never_auto: bool) -> PermissionRequest {
        PermissionRequest {
            id: id.into(),
            session_id: "s".into(),
            tool_name: "bash".into(),
            require_explicit_approval: explicit,
            never_auto_approve: never_auto,
            ..PermissionRequest::default()
        }
    }

    fn book() -> Approvals {
        let mut b = Approvals::new(Duration::from_secs(60));
        b.register_permission(perm_req("p", false, false), T0);
        b.register_permission(perm_req("e", true, false), T0);
        b.register_permission(perm_req("n", false, true), T0);
        b.register_question(
            QuestionRequest {
                id: "q".into(),
                session_id: "s".into(),
                questions: vec![],
            },
            T0,
        );
        b
    }

    #[test]
    fn only_an_explicit_approval_approves() {
        let mut b = book();
        assert_eq!(b.resolution("p"), None);
        assert!(!b.resolution("p").is_some_and(Resolution::is_approved));
        assert_eq!(
            b.decide("p", Decision::Approve, Origin::Explicit),
            Ok(Resolution::Approved)
        );
        assert!(b.resolution("p").is_some_and(Resolution::is_approved));
        assert!(b.resolution("e").is_none(), "other requests stay pending");
    }

    #[test]
    fn decisions_that_do_not_fit_or_repeat_are_refused_and_change_nothing() {
        let mut b = book();
        assert_eq!(
            b.decide("q", Decision::Approve, Origin::Explicit),
            Err(Refused::WrongKind)
        );
        assert_eq!(
            b.decide("q", Decision::ApproveSession, Origin::Explicit),
            Err(Refused::WrongKind)
        );
        assert_eq!(
            b.decide("p", Decision::Answer(vec![]), Origin::Explicit),
            Err(Refused::WrongKind)
        );
        assert_eq!(
            b.decide("zz", Decision::Approve, Origin::Explicit),
            Err(Refused::Unknown)
        );
        assert!(b.is_pending("q") && b.is_pending("p"));
        assert!(b.decide("p", Decision::Deny, Origin::Explicit).is_ok());
        assert_eq!(
            b.decide("p", Decision::Approve, Origin::Explicit),
            Err(Refused::AlreadyResolved)
        );
        assert!(
            !b.resolution("p").is_some_and(Resolution::is_approved),
            "a denial is final"
        );
    }

    #[test]
    fn flagged_requests_refuse_every_shortcut() {
        for id in ["e", "n"] {
            assert!(b_flagged(id));
            for origin in [Origin::Voice, Origin::Auto] {
                let mut b = book();
                assert_eq!(
                    b.decide(id, Decision::Approve, origin),
                    Err(Refused::NeedsExplicit),
                    "{id} {origin:?}"
                );
                assert!(b.is_pending(id), "still pending");
            }
            let mut b = book();
            for origin in [Origin::Explicit, Origin::Voice, Origin::Auto] {
                assert_eq!(
                    b.decide(id, Decision::ApproveSession, origin),
                    Err(Refused::SessionGrantNotAllowed)
                );
            }
            assert_eq!(
                b.decide(id, Decision::Approve, Origin::Explicit),
                Ok(Resolution::Approved)
            );
        }
    }

    fn b_flagged(id: &str) -> bool {
        book().is_flagged(id)
    }

    #[test]
    fn unflagged_session_grant_needs_an_explicit_origin() {
        let mut b = book();
        assert_eq!(
            b.decide("p", Decision::ApproveSession, Origin::Voice),
            Err(Refused::NeedsExplicit)
        );
        assert_eq!(
            b.decide("p", Decision::ApproveSession, Origin::Auto),
            Err(Refused::NeedsExplicit)
        );
        assert_eq!(
            b.decide("p", Decision::ApproveSession, Origin::Explicit),
            Ok(Resolution::ApprovedForSession)
        );
        // Voice may approve an unflagged request once (the decision layer owns that policy).
        let mut b = book();
        assert_eq!(
            b.decide("p", Decision::Approve, Origin::Voice),
            Ok(Resolution::Approved)
        );
    }

    #[test]
    fn timeout_denies_and_a_late_approval_does_nothing() {
        let mut b = book();
        assert!(b.expire(T0 + Duration::from_secs(59)).is_empty());
        let gone = b.expire(T0 + Duration::from_secs(60));
        assert_eq!(gone, vec!["e", "n", "p", "q"]);
        assert_eq!(
            b.resolution("p"),
            Some(&Resolution::Denied(DenyReason::Timeout))
        );
        assert_eq!(
            b.resolution("q"),
            Some(&Resolution::Cancelled(DenyReason::Timeout))
        );
        assert_eq!(
            b.decide("p", Decision::Approve, Origin::Explicit),
            Err(Refused::AlreadyResolved)
        );
        assert!(!b.resolution("p").is_some_and(Resolution::is_approved));
    }

    #[test]
    fn every_denial_reason_resolves_pending_as_denied_and_keeps_approved_ones() {
        for reason in [
            DenyReason::User,
            DenyReason::Timeout,
            DenyReason::PanelClosed,
            DenyReason::SessionSwitch,
            DenyReason::Quit,
            DenyReason::Cancelled,
            DenyReason::ServerExpired,
        ] {
            let mut b = book();
            let _ = b.decide("p", Decision::Approve, Origin::Explicit);
            let denied = b.deny_all(reason);
            assert_eq!(denied, vec!["e", "n", "q"], "{reason:?}");
            assert_eq!(b.resolution("p"), Some(&Resolution::Approved));
            for id in ["e", "n"] {
                assert_eq!(b.resolution(id), Some(&Resolution::Denied(reason)));
                assert!(!b.resolution(id).is_some_and(Resolution::is_approved));
            }
            assert_eq!(b.resolution("q"), Some(&Resolution::Cancelled(reason)));
            assert!(b.pending_ids().is_empty());
            assert_eq!(
                b.decide("e", Decision::Approve, Origin::Explicit),
                Err(Refused::AlreadyResolved)
            );
            assert!(!reason.as_str().is_empty());
        }
    }

    #[test]
    fn single_deny_by_reason() {
        let mut b = book();
        assert!(b.deny("p", DenyReason::PanelClosed));
        assert!(!b.deny("p", DenyReason::Quit), "not pending anymore");
        assert!(!b.deny("nope", DenyReason::Quit));
        assert_eq!(
            b.resolution("p"),
            Some(&Resolution::Denied(DenyReason::PanelClosed))
        );
    }

    #[test]
    fn outbox_never_turns_a_non_approval_into_allow() {
        let mut b = book();
        let _ = b.decide("p", Decision::Approve, Origin::Explicit);
        b.deny_all(DenyReason::Quit);
        let out = b.outbox();
        assert_eq!(out.len(), 4);
        for o in &out {
            match o {
                Outgoing::Permission { id, action, .. } if id == "p" => {
                    assert_eq!(*action, PermissionAction::Allow)
                }
                Outgoing::Permission { action, .. } => assert_eq!(*action, PermissionAction::Deny),
                Outgoing::Question {
                    cancelled, answers, ..
                } => assert!(*cancelled && answers.is_empty()),
            }
        }
        for id in ["p", "e", "n", "q"] {
            b.mark_delivered(id);
        }
        assert!(b.outbox().is_empty(), "delivered once");
    }

    #[test]
    fn pending_requests_produce_no_outgoing_answer() {
        assert!(book().outbox().is_empty());
    }

    #[test]
    fn replay_does_not_reset_a_resolved_request() {
        let mut b = book();
        let _ = b.decide("p", Decision::Deny, Origin::Explicit);
        b.register_permission(perm_req("p", false, false), T0 + Duration::from_secs(5));
        assert_eq!(
            b.resolution("p"),
            Some(&Resolution::Denied(DenyReason::User))
        );
    }

    #[test]
    fn deadlines_track_pending_only() {
        let mut b = Approvals::new(Duration::from_secs(5));
        assert!(b.next_deadline().is_none());
        b.register_permission(perm_req("a", false, false), Duration::from_secs(1));
        b.register_permission(perm_req("b", false, false), Duration::from_secs(3));
        assert_eq!(b.next_deadline(), Some(Duration::from_secs(6)));
        b.deny("a", DenyReason::User);
        assert_eq!(b.next_deadline(), Some(Duration::from_secs(8)));
    }

    #[test]
    fn answered_question_goes_out_with_its_answers() {
        let mut b = book();
        let ans = vec![QuestionAnswer {
            question_id: "q1".into(),
            selected: vec!["main".into()],
            other_text: String::new(),
        }];
        assert!(
            b.decide("q", Decision::Answer(ans.clone()), Origin::Explicit)
                .is_ok()
        );
        let out = b.outbox();
        assert!(out.iter().any(|o| matches!(o, Outgoing::Question { id, answers, cancelled: false, .. } if id == "q" && *answers == ans)));
    }
}
