//! Fixed-size ring that keeps the last N seconds of 16 kHz audio (so the start of an
//! utterance spoken before the wake word / key press is not lost).

/// Ring of the most recent samples.
#[derive(Debug, Clone)]
pub struct PreRoll {
    buf: Vec<f32>,
    /// Next write position.
    head: usize,
    filled: usize,
}

impl PreRoll {
    pub fn new(seconds: f32, rate: u32) -> Self {
        let n = ((seconds * rate as f32) as usize).max(1);
        Self {
            buf: vec![0.0; n],
            head: 0,
            filled: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub fn len(&self) -> usize {
        self.filled
    }

    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }

    pub fn push(&mut self, samples: &[f32]) {
        let cap = self.buf.len();
        // Only the last `cap` samples can survive.
        let samples = &samples[samples.len().saturating_sub(cap)..];
        for &s in samples {
            self.buf[self.head] = s;
            self.head = (self.head + 1) % cap;
        }
        self.filled = (self.filled + samples.len()).min(cap);
    }

    /// Stored samples, oldest first.
    pub fn snapshot(&self) -> Vec<f32> {
        let cap = self.buf.len();
        let start = (self.head + cap - self.filled) % cap;
        (0..self.filled)
            .map(|i| self.buf[(start + i) % cap])
            .collect()
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.filled = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_samples_in_order() {
        let mut p = PreRoll::new(1.0, 10); // capacity 10
        assert!(p.is_empty());
        p.push(&[1.0, 2.0, 3.0]);
        assert_eq!(p.snapshot(), vec![1.0, 2.0, 3.0]);
        let more: Vec<f32> = (4..=14).map(|v| v as f32).collect();
        p.push(&more);
        assert_eq!(p.len(), 10);
        assert_eq!(p.snapshot(), (5..=14).map(|v| v as f32).collect::<Vec<_>>());
    }

    #[test]
    fn oversized_push_and_clear() {
        let mut p = PreRoll::new(0.5, 10); // capacity 5
        p.push(&(0..100).map(|v| v as f32).collect::<Vec<_>>());
        assert_eq!(p.snapshot(), vec![95.0, 96.0, 97.0, 98.0, 99.0]);
        p.clear();
        assert!(p.snapshot().is_empty());
        p.push(&[7.0]);
        assert_eq!(p.snapshot(), vec![7.0]);
    }

    #[test]
    fn two_seconds_at_16k() {
        let p = PreRoll::new(2.0, 16_000);
        assert_eq!(p.capacity(), 32_000);
    }
}
