//! Per-stage latency of the voice path (PDA-T-0096).
//!
//! A [`Turn`] collects timestamps as an utterance moves release → PCM ready → transcript →
//! decision → API call → first audio; finishing it logs one `tracing` event with every stage
//! and feeds the [`Stats`] used by the debug panel (P50/P95 over the last samples).

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

/// A stage of the path; the duration of a stage is measured from the previous mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stage {
    /// Key release → the utterance PCM is assembled.
    PcmReady,
    /// PCM ready → transcript received from the STT worker.
    Transcript,
    /// Transcript → policy outcome (System One cascade).
    Decision,
    /// Outcome → the Pando API answered (command or first chat event).
    Api,
    /// API → first audio sample of the spoken answer played.
    FirstAudio,
}

impl Stage {
    /// Every stage in order.
    pub const ALL: [Stage; 5] = [
        Stage::PcmReady,
        Stage::Transcript,
        Stage::Decision,
        Stage::Api,
        Stage::FirstAudio,
    ];

    /// Stable name for logs and the panel.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PcmReady => "pcm_ready",
            Self::Transcript => "transcript",
            Self::Decision => "decision",
            Self::Api => "api",
            Self::FirstAudio => "first_audio",
        }
    }
}

/// Timestamps of one utterance.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Turn {
    last_ms: Option<u64>,
    stages: Vec<(Stage, u64)>,
}

impl Turn {
    /// Starts at the key release.
    #[must_use]
    pub fn started(release_ms: u64) -> Self {
        Self {
            last_ms: Some(release_ms),
            stages: Vec::new(),
        }
    }

    /// Marks the end of `stage` at `now_ms`.
    pub fn mark(&mut self, stage: Stage, now_ms: u64) {
        let from = self.last_ms.unwrap_or(now_ms);
        self.stages.push((stage, now_ms.saturating_sub(from)));
        self.last_ms = Some(now_ms);
    }

    /// Recorded `(stage, duration ms)` pairs.
    #[must_use]
    pub fn stages(&self) -> &[(Stage, u64)] {
        &self.stages
    }

    /// Sum of every recorded stage.
    #[must_use]
    pub fn total_ms(&self) -> u64 {
        self.stages.iter().map(|(_, d)| d).sum()
    }

    /// One-line summary for logs: `pcm_ready=3ms transcript=210ms ...`.
    #[must_use]
    pub fn summary(&self) -> String {
        self.stages
            .iter()
            .map(|(s, d)| format!("{}={d}ms", s.name()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Rolling window per stage.
#[derive(Debug, Clone)]
pub struct Stats {
    window: usize,
    samples: HashMap<Stage, VecDeque<u64>>,
}

impl Default for Stats {
    fn default() -> Self {
        Self::new(200)
    }
}

/// Percentiles of one stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Percentiles {
    /// Median.
    pub p50: u64,
    /// 95th percentile (nearest rank).
    pub p95: u64,
    /// Samples considered.
    pub n: usize,
}

impl Stats {
    /// Keeps the last `window` samples of each stage.
    #[must_use]
    pub fn new(window: usize) -> Self {
        Self {
            window: window.max(1),
            samples: HashMap::new(),
        }
    }

    /// Records a finished turn and logs it.
    pub fn finish(&mut self, turn: &Turn) {
        for (stage, ms) in turn.stages() {
            let q = self.samples.entry(*stage).or_default();
            if q.len() == self.window {
                q.pop_front();
            }
            q.push_back(*ms);
        }
        tracing::info!(total_ms = turn.total_ms(), "voice turn: {}", turn.summary());
    }

    /// Percentiles of `stage`, if any sample exists.
    #[must_use]
    pub fn percentiles(&self, stage: Stage) -> Option<Percentiles> {
        let q = self.samples.get(&stage).filter(|q| !q.is_empty())?;
        let mut v: Vec<u64> = q.iter().copied().collect();
        v.sort_unstable();
        let rank = |p: f64| {
            let idx = (p * v.len() as f64).ceil() as usize;
            v[idx.clamp(1, v.len()) - 1]
        };
        Some(Percentiles {
            p50: rank(0.5),
            p95: rank(0.95),
            n: v.len(),
        })
    }

    /// Debug-panel lines: `stage  p50 .. ms  p95 .. ms  (n)`.
    #[must_use]
    pub fn report(&self) -> Vec<String> {
        Stage::ALL
            .iter()
            .filter_map(|s| {
                self.percentiles(*s).map(|p| {
                    format!(
                        "{:<12} p50 {:>5} ms  p95 {:>5} ms  (n={})",
                        s.name(),
                        p.p50,
                        p.p95,
                        p.n
                    )
                })
            })
            .collect()
    }
}

/// Convenience for converting a duration to whole milliseconds.
#[must_use]
pub fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_measured_from_the_previous_mark() {
        let mut t = Turn::started(1_000);
        t.mark(Stage::PcmReady, 1_003);
        t.mark(Stage::Transcript, 1_213);
        t.mark(Stage::Decision, 1_300);
        assert_eq!(
            t.stages(),
            &[
                (Stage::PcmReady, 3),
                (Stage::Transcript, 210),
                (Stage::Decision, 87)
            ]
        );
        assert_eq!(t.total_ms(), 300);
        assert_eq!(t.summary(), "pcm_ready=3ms transcript=210ms decision=87ms");
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut s = Stats::new(100);
        for i in 1..=100_u64 {
            let mut t = Turn::started(0);
            t.mark(Stage::Transcript, i);
            s.finish(&t);
        }
        let p = s.percentiles(Stage::Transcript).expect("samples");
        assert_eq!((p.p50, p.p95, p.n), (50, 95, 100));
        assert!(s.percentiles(Stage::Api).is_none());
        assert_eq!(s.report().len(), 1);
    }

    #[test]
    fn the_window_forgets_old_samples() {
        let mut s = Stats::new(3);
        for i in [1_000, 1, 2, 3] {
            let mut t = Turn::started(0);
            t.mark(Stage::Api, i);
            s.finish(&t);
        }
        assert_eq!(s.percentiles(Stage::Api).map(|p| p.p95), Some(3));
    }
}
