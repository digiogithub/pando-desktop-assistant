//! Pause-aware splitting of long audio (ADR 0008: Whistle has a latency cliff above 5 s).

use voice_proto::STT_SAMPLE_RATE;

/// Frame length used for the energy scan (20 ms).
const FRAME: usize = (STT_SAMPLE_RATE as usize) / 50;

/// Split `pcm` into pieces of at most `max_samples`, cutting at the quietest 20 ms frame in the
/// last third of each window so words are rarely cut. Pieces cover the input exactly, in order.
pub fn split_at_pauses(pcm: &[f32], max_samples: usize) -> Vec<&[f32]> {
    let max_samples = max_samples.max(FRAME * 3);
    let mut out = Vec::new();
    let mut start = 0;
    while pcm.len() - start > max_samples {
        let window = &pcm[start..start + max_samples];
        let lo = max_samples - max_samples / 3;
        let mut best = (f32::MAX, max_samples);
        let mut i = lo;
        while i + FRAME <= max_samples {
            let e: f32 = window[i..i + FRAME].iter().map(|s| s * s).sum();
            if e < best.0 {
                best = (e, i + FRAME / 2);
            }
            i += FRAME;
        }
        out.push(&pcm[start..start + best.1]);
        start += best.1;
    }
    if start < pcm.len() {
        out.push(&pcm[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_input_is_one_piece() {
        let pcm = vec![0.1; 16_000 * 3];
        assert_eq!(split_at_pauses(&pcm, 80_000).len(), 1);
        assert!(split_at_pauses(&[], 80_000).is_empty());
    }

    #[test]
    fn pieces_cover_input_and_respect_the_limit() {
        let pcm: Vec<f32> = (0..16_000 * 17)
            .map(|i| ((i as f32) * 0.05).sin() * 0.5)
            .collect();
        let max = 16_000 * 5;
        let parts = split_at_pauses(&pcm, max);
        assert!(parts.len() >= 4);
        assert!(parts.iter().all(|p| p.len() <= max));
        assert_eq!(parts.iter().map(|p| p.len()).sum::<usize>(), pcm.len());
    }

    #[test]
    fn cuts_at_the_pause() {
        let mut pcm = vec![0.5f32; 16_000 * 8];
        // silence at 4.0..4.2 s (inside the last third of a 5 s window)
        for s in &mut pcm[64_000..67_200] {
            *s = 0.0;
        }
        let parts = split_at_pauses(&pcm, 16_000 * 5);
        let cut = parts[0].len();
        assert!((64_000..67_200).contains(&cut), "cut at {cut}");
    }
}
