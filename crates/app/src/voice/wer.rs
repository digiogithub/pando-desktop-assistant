//! Word error rate for the STT quality gate (PDA-T-0096). The gate itself is a local script
//! (`scripts/wer-gate.sh`), not CI.

/// Lowercases, strips punctuation and splits into words (accents are kept: they matter in Spanish).
#[must_use]
pub fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word error rate in `0.0..` (substitutions + deletions + insertions over reference words).
#[must_use]
pub fn wer(reference: &str, hypothesis: &str) -> f64 {
    let r = words(reference);
    let h = words(hypothesis);
    if r.is_empty() {
        return if h.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for (i, rw) in r.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, hw) in h.iter().enumerate() {
            let sub = prev[j] + usize::from(rw != hw);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[h.len()] as f64 / r.len() as f64
}

/// Result of comparing a run with the stored baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateVerdict {
    /// Baseline WER, percent.
    pub baseline_pct: f64,
    /// Current WER, percent.
    pub current_pct: f64,
    /// Passed when the current WER is at most `tolerance_points` worse.
    pub passed: bool,
}

/// The gate: fails when the WER worsens by more than `tolerance_points` percentage points.
#[must_use]
pub fn gate(baseline: f64, current: f64, tolerance_points: f64) -> GateVerdict {
    let (b, c) = (baseline * 100.0, current * 100.0);
    GateVerdict {
        baseline_pct: b,
        current_pct: c,
        passed: c - b <= tolerance_points + 1e-9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_zero_and_normalisation_ignores_case_and_punctuation() {
        assert_eq!(wer("Cancela la migración.", "cancela la migración"), 0.0);
    }

    #[test]
    fn counts_substitutions_insertions_and_deletions() {
        assert!((wer("uno dos tres cuatro", "uno dos tres") - 0.25).abs() < 1e-9);
        assert!((wer("uno dos", "uno tres dos") - 0.5).abs() < 1e-9);
        assert!((wer("uno dos", "uno tres") - 0.5).abs() < 1e-9);
        assert_eq!(wer("", ""), 0.0);
        assert_eq!(wer("", "algo"), 1.0);
    }

    #[test]
    fn accents_matter() {
        assert!(wer("migración", "migracion") > 0.0);
    }

    #[test]
    fn gate_tolerates_two_points() {
        assert!(gate(0.10, 0.12, 2.0).passed);
        assert!(!gate(0.10, 0.1201, 2.0).passed);
        assert!(gate(0.10, 0.05, 2.0).passed);
    }
}
