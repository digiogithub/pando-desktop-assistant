//! Usage accounting (PDA-T-0078): every field of `token_usage`, estimated vs confirmed.
//!
//! Observed behaviour (Pando v1.3.0, see the recorded fixtures): `token_usage` events carry
//! **cumulative session totals**. Confirmed events (`estimated == false`) match the session's
//! `PromptTokens`/`CompletionTokens`/`Cache*`/`Cost`; estimated ones are provisional and
//! interleave with them (cost 0). So the authoritative number is the latest confirmed event;
//! a provisional one is shown only while it is newer.

use crate::chat::{Session, TokenUsage};

/// One reading of the counters.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageSnapshot {
    /// Prompt tokens.
    pub prompt_tokens: i64,
    /// Completion tokens.
    pub completion_tokens: i64,
    /// Cache read tokens.
    pub cache_read_tokens: i64,
    /// Cache creation tokens.
    pub cache_creation_tokens: i64,
    /// Reasoning tokens.
    pub reasoning_tokens: i64,
    /// Cost in USD.
    pub cost: f64,
    /// Context window of the model (0 when unknown).
    pub context_window: i64,
    /// The numbers are estimates (the breakdown is not meaningful).
    pub estimated: bool,
}

impl From<&TokenUsage> for UsageSnapshot {
    fn from(u: &TokenUsage) -> Self {
        Self {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            cache_read_tokens: u.cache_read_tokens,
            cache_creation_tokens: u.cache_creation_tokens,
            reasoning_tokens: u.reasoning_tokens,
            cost: u.cost,
            context_window: u.context_window,
            estimated: u.estimated,
        }
    }
}

impl From<&Session> for UsageSnapshot {
    fn from(s: &Session) -> Self {
        Self {
            prompt_tokens: s.prompt_tokens,
            completion_tokens: s.completion_tokens,
            cache_read_tokens: s.cache_read_tokens,
            cache_creation_tokens: s.cache_creation_tokens,
            reasoning_tokens: s.reasoning_tokens,
            cost: s.cost,
            context_window: 0,
            estimated: false,
        }
    }
}

/// The usage of one session as the panel shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageState {
    confirmed: Option<UsageSnapshot>,
    provisional: Option<UsageSnapshot>,
    session: Option<UsageSnapshot>,
}

impl UsageState {
    /// Feeds a `token_usage` event.
    pub fn apply(&mut self, u: &TokenUsage) {
        let snap = UsageSnapshot::from(u);
        if u.estimated {
            self.provisional = Some(snap);
        } else {
            self.confirmed = Some(snap);
            self.provisional = None;
        }
    }

    /// Feeds the session totals from `GET /sessions/{id}` (used until a live event arrives).
    pub fn set_session_totals(&mut self, s: &Session) {
        self.session = Some(UsageSnapshot::from(s));
    }

    /// What to display now: a newer provisional reading, else the latest confirmed one, else the
    /// loaded session totals.
    pub fn current(&self) -> Option<UsageSnapshot> {
        self.provisional.or(self.confirmed).or(self.session)
    }

    /// The authoritative totals (never an estimate), if known.
    pub fn totals(&self) -> Option<UsageSnapshot> {
        self.confirmed.or(self.session)
    }

    /// The displayed reading is an estimate.
    pub fn is_estimated(&self) -> bool {
        self.provisional.is_some()
    }

    /// Context window of the current model, `0` when unknown.
    pub fn context_window(&self) -> i64 {
        self.current().map_or(0, |u| u.context_window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(prompt: i64, cost: f64, estimated: bool) -> TokenUsage {
        TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: 7,
            cache_read_tokens: 3,
            cache_creation_tokens: 2,
            reasoning_tokens: 5,
            context_window: 1000,
            cost,
            estimated,
            ..TokenUsage::default()
        }
    }

    #[test]
    fn keeps_every_field() {
        let mut u = UsageState::default();
        u.apply(&ev(10, 0.5, false));
        let c = u.current().unwrap_or_default();
        assert_eq!(
            (
                c.prompt_tokens,
                c.completion_tokens,
                c.cache_read_tokens,
                c.cache_creation_tokens,
                c.reasoning_tokens,
                c.context_window
            ),
            (10, 7, 3, 2, 5, 1000)
        );
        assert_eq!(c.cost, 0.5);
        assert!(!c.estimated && !u.is_estimated());
    }

    #[test]
    fn provisional_is_shown_only_while_newer_than_confirmed() {
        let mut u = UsageState::default();
        u.apply(&ev(10, 0.5, false));
        u.apply(&ev(99, 0.0, true));
        assert!(u.is_estimated());
        assert_eq!(u.current().map(|c| c.prompt_tokens), Some(99));
        assert_eq!(
            u.totals().map(|c| c.prompt_tokens),
            Some(10),
            "totals ignore estimates"
        );
        u.apply(&ev(20, 0.7, false));
        assert!(!u.is_estimated());
        assert_eq!(u.current().map(|c| c.prompt_tokens), Some(20));
    }

    #[test]
    fn session_totals_back_the_panel_until_live_data_arrives() {
        let mut u = UsageState::default();
        assert!(u.current().is_none());
        let s = Session {
            prompt_tokens: 61,
            completion_tokens: 9,
            cache_read_tokens: 4,
            cache_creation_tokens: 1,
            reasoning_tokens: 2,
            cost: 0.25,
            ..Session::default()
        };
        u.set_session_totals(&s);
        let c = u.current().unwrap_or_default();
        assert_eq!(
            (
                c.cache_read_tokens,
                c.cache_creation_tokens,
                c.reasoning_tokens
            ),
            (4, 1, 2)
        );
        u.apply(&ev(70, 0.3, false));
        assert_eq!(u.current().map(|c| c.prompt_tokens), Some(70));
    }
}
