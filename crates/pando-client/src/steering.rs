//! Steering queue and Stop (PDA-T-0073), pure state.
//!
//! With a run active, a new message is *steering*: the UI shows an "En cola" chip and calls
//! `POST /sessions/{id}/steer`. The chip stays queued until `steering_injected` (then the model
//! inserts the user text). If the run ended in the meantime (`409 not_busy`, or `done` before
//! the injection) the text is **never lost**: it is handed back to be sent as a normal prompt.

use crate::chat::Goal;
use crate::client::PandoClient;
use crate::error::{Error, Result};

/// Identifies a chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChipId(pub u64);

/// State of a chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChipState {
    /// The steer request is in flight.
    Sending,
    /// Accepted by Pando, waiting for `steering_injected` ("En cola").
    Queued,
    /// The steer request failed; the text is kept so the user can retry or discard.
    Failed(String),
}

/// A queued steering message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteerChip {
    /// Id.
    pub id: ChipId,
    /// Text.
    pub text: String,
    /// State.
    pub state: ChipState,
}

/// What to do with a submitted message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendPlan {
    /// No run is active: send as a normal prompt via `chat_stream`.
    Prompt(String),
    /// A run is active: call `steer`, then report the outcome on the queue.
    Steer {
        /// The chip showing it.
        chip: ChipId,
        /// Text to steer with.
        text: String,
    },
}

/// What Stop must do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopPlan {
    /// `POST /sessions/{id}/cancel`.
    pub cancel_run: bool,
    /// `POST /sessions/{id}/goal/cancel` (a goal is active).
    pub cancel_goal: bool,
}

/// Result of [`send_steer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteerResult {
    /// Queued server side.
    Accepted,
    /// 409 `not_busy`: the run ended; send it as a normal prompt.
    NotBusy,
}

/// The chips of one session.
#[derive(Debug, Clone, Default)]
pub struct SteeringQueue {
    chips: Vec<SteerChip>,
    next: u64,
}

impl SteeringQueue {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Decides what to do with `text` given whether a run is active.
    pub fn submit(&mut self, text: &str, run_active: bool) -> SendPlan {
        if !run_active {
            return SendPlan::Prompt(text.to_owned());
        }
        self.next += 1;
        let id = ChipId(self.next);
        self.chips.push(SteerChip {
            id,
            text: text.to_owned(),
            state: ChipState::Sending,
        });
        SendPlan::Steer {
            chip: id,
            text: text.to_owned(),
        }
    }

    /// The chips to render, oldest first.
    pub fn chips(&self) -> &[SteerChip] {
        &self.chips
    }

    /// The steer call returned 202.
    pub fn steer_accepted(&mut self, chip: ChipId) {
        if let Some(c) = self.chips.iter_mut().find(|c| c.id == chip) {
            c.state = ChipState::Queued;
        }
    }

    /// The steer call returned 409 `not_busy`: drops the chip and returns its text to send as a
    /// normal prompt.
    pub fn steer_not_busy(&mut self, chip: ChipId) -> Option<String> {
        self.remove(chip)
    }

    /// The steer call failed for another reason; the text stays on a failed chip.
    pub fn steer_failed(&mut self, chip: ChipId, reason: &str) {
        if let Some(c) = self.chips.iter_mut().find(|c| c.id == chip) {
            c.state = ChipState::Failed(reason.to_owned());
        }
    }

    /// The user discards a chip (typically a failed one).
    pub fn discard(&mut self, chip: ChipId) -> Option<String> {
        self.remove(chip)
    }

    /// `steering_injected`: removes the matching chip (same text, else the oldest queued one)
    /// and returns the text to insert into the transcript.
    pub fn injected(&mut self, message: &str) -> Option<String> {
        let pos = self
            .chips
            .iter()
            .position(|c| c.text == message && c.state != ChipState::Sending)
            .or_else(|| self.chips.iter().position(|c| c.state == ChipState::Queued));
        pos.map(|i| self.chips.remove(i).text)
    }

    /// The run ended: every chip that was never injected (queued or still sending) is removed
    /// and returned, oldest first, so the text can be sent as a normal prompt. Failed chips stay
    /// for the user to decide.
    pub fn run_ended(&mut self) -> Vec<String> {
        let mut orphaned = Vec::new();
        self.chips.retain(|c| {
            if matches!(c.state, ChipState::Queued | ChipState::Sending) {
                orphaned.push(c.text.clone());
                false
            } else {
                true
            }
        });
        orphaned
    }

    fn remove(&mut self, chip: ChipId) -> Option<String> {
        let i = self.chips.iter().position(|c| c.id == chip)?;
        Some(self.chips.remove(i).text)
    }
}

/// What Stop must do given the goal (cancel the goal too when one is active).
pub fn stop_plan(run_active: bool, goal: Option<&Goal>) -> StopPlan {
    StopPlan {
        cancel_run: run_active,
        cancel_goal: goal.is_some_and(|g| g.status == "running"),
    }
}

/// Calls `/steer`, mapping `409 not_busy` to [`SteerResult::NotBusy`].
pub async fn send_steer(client: &PandoClient, session_id: &str, text: &str) -> Result<SteerResult> {
    match client.steer(session_id, text).await {
        Ok(_) => Ok(SteerResult::Accepted),
        Err(Error::Server { status: 409, .. }) => Ok(SteerResult::NotBusy),
        Err(e) => Err(e),
    }
}

/// Executes a [`StopPlan`]. A 404 on goal cancel ("no active goal") is not an error.
pub async fn execute_stop(client: &PandoClient, session_id: &str, plan: StopPlan) -> Result<()> {
    if plan.cancel_run {
        client.cancel(session_id).await?;
    }
    if plan.cancel_goal {
        match client.goal_cancel(session_id).await {
            Ok(_) | Err(Error::Server { status: 404, .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_session_sends_a_normal_prompt() {
        let mut q = SteeringQueue::new();
        assert_eq!(q.submit("hi", false), SendPlan::Prompt("hi".into()));
        assert!(q.chips().is_empty());
    }

    #[test]
    fn steer_flow_queues_then_injects() {
        let mut q = SteeringQueue::new();
        let SendPlan::Steer { chip, text } = q.submit("also docs", true) else {
            panic!("steer")
        };
        assert_eq!(
            (text.as_str(), &q.chips()[0].state),
            ("also docs", &ChipState::Sending)
        );
        q.steer_accepted(chip);
        assert_eq!(q.chips()[0].state, ChipState::Queued);
        assert_eq!(q.injected("also docs"), Some("also docs".into()));
        assert!(q.chips().is_empty());
    }

    #[test]
    fn not_busy_hands_the_text_back() {
        let mut q = SteeringQueue::new();
        let SendPlan::Steer { chip, .. } = q.submit("late", true) else {
            panic!("steer")
        };
        assert_eq!(q.steer_not_busy(chip), Some("late".into()));
        assert!(q.chips().is_empty());
    }

    #[test]
    fn steer_done_race_loses_nothing() {
        let mut q = SteeringQueue::new();
        let SendPlan::Steer { chip: a, .. } = q.submit("one", true) else {
            panic!("steer")
        };
        let SendPlan::Steer { chip: b, .. } = q.submit("two", true) else {
            panic!("steer")
        };
        q.steer_accepted(a); // b is still in flight when the run ends
        assert_eq!(q.run_ended(), vec!["one", "two"]);
        assert!(q.chips().is_empty());
        let _ = b;
    }

    #[test]
    fn failed_chip_survives_run_end_until_discarded() {
        let mut q = SteeringQueue::new();
        let SendPlan::Steer { chip, .. } = q.submit("x", true) else {
            panic!("steer")
        };
        q.steer_failed(chip, "boom");
        assert!(q.run_ended().is_empty());
        assert_eq!(q.chips()[0].state, ChipState::Failed("boom".into()));
        assert_eq!(q.discard(chip), Some("x".into()));
    }

    #[test]
    fn injection_matches_by_text_then_falls_back_to_oldest_queued() {
        let mut q = SteeringQueue::new();
        for t in ["a", "b"] {
            let SendPlan::Steer { chip, .. } = q.submit(t, true) else {
                panic!("steer")
            };
            q.steer_accepted(chip);
        }
        assert_eq!(q.injected("b"), Some("b".into()));
        assert_eq!(q.injected("something else"), Some("a".into()));
        assert_eq!(q.injected("none left"), None);
    }

    #[test]
    fn stop_cancels_the_goal_only_when_one_is_active() {
        let active = Goal {
            status: "running".into(),
            ..Goal::default()
        };
        let done = Goal {
            status: "completed".into(),
            ..Goal::default()
        };
        assert_eq!(
            stop_plan(true, Some(&active)),
            StopPlan {
                cancel_run: true,
                cancel_goal: true
            }
        );
        assert_eq!(
            stop_plan(true, Some(&done)),
            StopPlan {
                cancel_run: true,
                cancel_goal: false
            }
        );
        assert_eq!(
            stop_plan(true, None),
            StopPlan {
                cancel_run: true,
                cancel_goal: false
            }
        );
        assert!(!stop_plan(false, Some(&active)).cancel_run);
    }
}
