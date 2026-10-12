//! Speech queue (PDA-T-0111, `PDA-SP-0007.R4`): priorities, no overlap, waits while the user is
//! speaking, mute, skip and barge-in. Pure state machine: the caller feeds events in and acts on
//! the returned [`Action`]s; no clock, no audio.

use std::collections::VecDeque;

/// Utterance priority: a direct reply beats a requested conclusion beats a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Notification = 0,
    Conclusion = 1,
    Direct = 2,
}

/// Something to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utterance {
    pub id: u64,
    pub priority: Priority,
    pub text: String,
    /// Cell / agent that produced it (for the "tengo algo" pulse).
    pub source: Option<String>,
}

impl Utterance {
    pub fn new(id: u64, priority: Priority, text: impl Into<String>) -> Self {
        Self {
            id,
            priority,
            text: text.into(),
            source: None,
        }
    }

    pub fn from_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// What the caller must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start speaking this utterance.
    Speak(Utterance),
    /// Stop the audio of the utterance that is playing now.
    Stop { id: u64 },
}

/// Result of [`SpeechQueue::enqueue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enqueued {
    /// Accepted; will be spoken when its turn comes.
    Queued,
    /// Muted: show the text and pulse the cell, nothing is queued.
    Muted(Utterance),
}

/// Queue state.
#[derive(Debug)]
pub struct SpeechQueue {
    pending: VecDeque<Utterance>,
    speaking: Option<Utterance>,
    listening: bool,
    muted: bool,
    max_notifications: usize,
}

impl Default for SpeechQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeechQueue {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            speaking: None,
            listening: false,
            muted: false,
            max_notifications: 8,
        }
    }

    /// Cap on queued notifications; the oldest are dropped first.
    pub fn with_max_notifications(mut self, n: usize) -> Self {
        self.max_notifications = n;
        self
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn is_listening(&self) -> bool {
        self.listening
    }

    /// The utterance being spoken.
    pub fn speaking(&self) -> Option<&Utterance> {
        self.speaking.as_ref()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// True while idle with nothing queued or playing (the body can go back to Idle).
    pub fn is_idle(&self) -> bool {
        self.speaking.is_none() && self.pending.is_empty()
    }

    /// Add an utterance. When muted it is not queued: show it as text and pulse the cell.
    pub fn enqueue(&mut self, u: Utterance) -> Enqueued {
        if self.muted {
            return Enqueued::Muted(u);
        }
        self.pending.push_back(u);
        self.trim_notifications();
        Enqueued::Queued
    }

    fn trim_notifications(&mut self) {
        let count = self
            .pending
            .iter()
            .filter(|u| u.priority == Priority::Notification)
            .count();
        let mut excess = count.saturating_sub(self.max_notifications);
        self.pending.retain(|u| {
            if excess > 0 && u.priority == Priority::Notification {
                excess -= 1;
                false
            } else {
                true
            }
        });
    }

    /// The body entered or left Listening. Nothing is spoken while listening; an utterance that
    /// is playing when the user starts speaking is stopped (barge-in).
    pub fn set_listening(&mut self, listening: bool) -> Vec<Action> {
        self.listening = listening;
        if listening {
            return self.interrupt();
        }
        self.poll().into_iter().collect()
    }

    /// Turn mute on or off. Muting stops what is playing and drops the queue.
    pub fn set_muted(&mut self, muted: bool) -> Vec<Action> {
        self.muted = muted;
        if muted {
            let mut actions = Vec::new();
            if let Some(cur) = self.speaking.take() {
                actions.push(Action::Stop { id: cur.id });
            }
            self.pending.clear();
            actions
        } else {
            self.poll().into_iter().collect()
        }
    }

    /// "calla" / Esc: stop the current utterance and go on with the next one.
    pub fn skip(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if let Some(cur) = self.speaking.take() {
            actions.push(Action::Stop { id: cur.id });
        }
        actions.extend(self.poll());
        actions
    }

    /// "calla todo": stop the current utterance and forget the queue.
    pub fn skip_all(&mut self) -> Vec<Action> {
        self.pending.clear();
        self.skip()
    }

    /// Barge-in: stop the current utterance (its remaining sentences are discarded); the rest of the
    /// queue is kept and resumes when the user stops speaking.
    pub fn interrupt(&mut self) -> Vec<Action> {
        self.speaking
            .take()
            .map(|cur| Action::Stop { id: cur.id })
            .into_iter()
            .collect()
    }

    /// The audio of `id` finished (or was drained). Returns the next utterance to speak, if any.
    pub fn finished(&mut self, id: u64) -> Vec<Action> {
        if self.speaking.as_ref().is_some_and(|u| u.id == id) {
            self.speaking = None;
        }
        self.poll().into_iter().collect()
    }

    /// Next utterance if the queue may speak now.
    pub fn poll(&mut self) -> Option<Action> {
        if self.muted || self.listening || self.speaking.is_some() {
            return None;
        }
        // highest priority first, oldest first within a priority
        let mut best: Option<usize> = None;
        for (i, u) in self.pending.iter().enumerate() {
            if best.is_none_or(|b| u.priority > self.pending[b].priority) {
                best = Some(i);
            }
        }
        let u = self.pending.remove(best?)?;
        self.speaking = Some(u.clone());
        Some(Action::Speak(u))
    }

    /// Conclusions waiting because the user is speaking: drive the "tengo algo" pulse.
    pub fn has_pending_conclusion(&self) -> bool {
        self.pending
            .iter()
            .any(|u| u.priority >= Priority::Conclusion)
    }

    /// Sources of the waiting utterances (cells to pulse), without duplicates, in queue order.
    pub fn pending_sources(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for u in &self.pending {
            if let Some(s) = &u.source
                && !seen.contains(s)
            {
                seen.push(s.clone());
            }
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(id: u64, p: Priority) -> Utterance {
        Utterance::new(id, p, format!("texto {id}"))
    }

    fn speak_id(a: Option<&Action>) -> Option<u64> {
        match a {
            Some(Action::Speak(u)) => Some(u.id),
            _ => None,
        }
    }

    #[test]
    fn first_utterance_speaks_immediately_on_poll() {
        let mut q = SpeechQueue::new();
        assert_eq!(q.enqueue(u(1, Priority::Conclusion)), Enqueued::Queued);
        assert_eq!(speak_id(q.poll().as_ref()), Some(1));
        assert!(q.poll().is_none(), "never two at once");
        assert_eq!(q.speaking().map(|s| s.id), Some(1));
    }

    #[test]
    fn priority_order_then_fifo() {
        let mut q = SpeechQueue::new();
        q.set_listening(true);
        q.enqueue(u(1, Priority::Notification));
        q.enqueue(u(2, Priority::Conclusion));
        q.enqueue(u(3, Priority::Direct));
        q.enqueue(u(4, Priority::Conclusion));
        q.enqueue(u(5, Priority::Direct));
        let mut order = Vec::new();
        let mut acts = q.set_listening(false);
        while let Some(a) = acts.pop() {
            let id = speak_id(Some(&a)).expect("speak");
            order.push(id);
            acts = q.finished(id);
        }
        assert_eq!(order, vec![3, 5, 2, 4, 1]);
        assert!(q.is_idle());
    }

    #[test]
    fn waits_while_listening() {
        let mut q = SpeechQueue::new();
        q.set_listening(true);
        q.enqueue(u(1, Priority::Direct));
        assert!(q.poll().is_none());
        assert!(q.has_pending_conclusion());
        let acts = q.set_listening(false);
        assert_eq!(speak_id(acts.first()), Some(1));
        assert!(!q.has_pending_conclusion());
    }

    #[test]
    fn listening_interrupts_playback_and_resumes_after() {
        let mut q = SpeechQueue::new();
        q.enqueue(u(1, Priority::Conclusion));
        q.enqueue(u(2, Priority::Notification));
        assert_eq!(speak_id(q.poll().as_ref()), Some(1));
        let acts = q.set_listening(true);
        assert_eq!(acts, vec![Action::Stop { id: 1 }]);
        assert!(q.speaking().is_none());
        assert!(q.poll().is_none(), "silent while the user talks");
        let acts = q.set_listening(false);
        assert_eq!(
            speak_id(acts.first()),
            Some(2),
            "interrupted utterance is gone, next one resumes"
        );
    }

    #[test]
    fn mute_shows_text_and_plays_nothing() {
        let mut q = SpeechQueue::new();
        q.set_muted(true);
        let got = q.enqueue(u(1, Priority::Conclusion).from_source("cell-a"));
        assert!(
            matches!(got, Enqueued::Muted(ref x) if x.id == 1 && x.source.as_deref() == Some("cell-a"))
        );
        assert!(q.poll().is_none());
        assert!(q.is_empty());
    }

    #[test]
    fn muting_stops_current_and_clears() {
        let mut q = SpeechQueue::new();
        q.enqueue(u(1, Priority::Direct));
        q.enqueue(u(2, Priority::Direct));
        q.poll();
        let acts = q.set_muted(true);
        assert_eq!(acts, vec![Action::Stop { id: 1 }]);
        assert!(q.is_idle());
        let acts = q.set_muted(false);
        assert!(acts.is_empty());
        q.enqueue(u(3, Priority::Direct));
        assert_eq!(speak_id(q.poll().as_ref()), Some(3));
    }

    #[test]
    fn skip_moves_to_the_next() {
        let mut q = SpeechQueue::new();
        q.enqueue(u(1, Priority::Direct));
        q.enqueue(u(2, Priority::Direct));
        q.poll();
        let acts = q.skip();
        assert_eq!(acts[0], Action::Stop { id: 1 });
        assert_eq!(speak_id(acts.get(1)), Some(2));
    }

    #[test]
    fn skip_all_forgets_the_queue() {
        let mut q = SpeechQueue::new();
        for i in 1..=3 {
            q.enqueue(u(i, Priority::Conclusion));
        }
        q.poll();
        let acts = q.skip_all();
        assert_eq!(acts, vec![Action::Stop { id: 1 }]);
        assert!(q.is_idle());
    }

    #[test]
    fn skip_when_idle_is_a_no_op() {
        let mut q = SpeechQueue::new();
        assert!(q.skip().is_empty());
        assert!(q.interrupt().is_empty());
    }

    #[test]
    fn finished_with_a_stale_id_is_ignored() {
        let mut q = SpeechQueue::new();
        q.enqueue(u(1, Priority::Direct));
        q.poll();
        q.interrupt();
        q.enqueue(u(2, Priority::Direct));
        q.poll();
        let acts = q.finished(1);
        assert!(acts.is_empty());
        assert_eq!(q.speaking().map(|s| s.id), Some(2));
    }

    #[test]
    fn notifications_are_capped_oldest_first() {
        let mut q = SpeechQueue::new().with_max_notifications(2);
        q.set_listening(true);
        for i in 1..=4 {
            q.enqueue(u(i, Priority::Notification));
        }
        q.enqueue(u(9, Priority::Direct));
        assert_eq!(q.len(), 3);
        let mut ids = Vec::new();
        let mut acts = q.set_listening(false);
        while let Some(a) = acts.pop() {
            let id = speak_id(Some(&a)).expect("speak");
            ids.push(id);
            acts = q.finished(id);
        }
        assert_eq!(ids, vec![9, 3, 4]);
    }

    #[test]
    fn pending_sources_for_the_pulse() {
        let mut q = SpeechQueue::new();
        q.set_listening(true);
        q.enqueue(u(1, Priority::Conclusion).from_source("a"));
        q.enqueue(u(2, Priority::Conclusion).from_source("b"));
        q.enqueue(u(3, Priority::Notification).from_source("a"));
        assert_eq!(q.pending_sources(), vec!["a".to_string(), "b".to_string()]);
        assert!(q.has_pending_conclusion());
    }

    #[test]
    fn notification_only_is_not_a_pending_conclusion() {
        let mut q = SpeechQueue::new();
        q.set_listening(true);
        q.enqueue(u(1, Priority::Notification));
        assert!(!q.has_pending_conclusion());
    }

    #[test]
    fn higher_priority_waits_for_current_but_goes_next() {
        let mut q = SpeechQueue::new();
        q.enqueue(u(1, Priority::Notification));
        q.poll();
        q.enqueue(u(2, Priority::Conclusion));
        q.enqueue(u(3, Priority::Direct));
        assert!(q.poll().is_none(), "does not interrupt the current one");
        let acts = q.finished(1);
        assert_eq!(speak_id(acts.first()), Some(3));
    }
}
