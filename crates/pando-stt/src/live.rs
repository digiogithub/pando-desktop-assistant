//! Push-to-talk live transcription on top of one-shot `transcribe` (PDA-T-0095).
//!
//! Needle's own `stream_process` re-reads everything fed so far: a 0.75 s chunk costs 25 ms at the
//! start of an utterance and 800 ms after 13 s (measured, `examples/stream_latency.rs`), so the
//! worker falls behind real time and the final transcript waits for the backlog. One-shot decoding
//! costs ~20 ms per second of audio regardless of history, so the stream is cut at speech pauses:
//! each finished phrase is decoded once and committed while the user is still talking, and
//! releasing the key only decodes the phrase being spoken (usually the whole short command).

use voice_proto::{STT_SAMPLE_RATE, Word};

use crate::stt::{Stt, SttError, SttOpts, Transcript};

/// Analysis frame (20 ms).
const FRAME: usize = STT_SAMPLE_RATE as usize / 50;
/// Silence that ends a phrase (frames).
const PAUSE_FRAMES: usize = 18;
/// Frames of the silence kept after a cut so the next phrase starts clean.
const KEEP_FRAMES: usize = 2;
/// Longest undecoded audio held before it is cut at its quietest point (keeps partials cheap).
const MAX_PENDING: usize = STT_SAMPLE_RATE as usize * 4;
/// Loudest frame RMS below which a buffer counts as silence and is not sent to the engine.
const AUDIBLE_RMS: f32 = 0.005;

fn rms(frame: &[f32]) -> f32 {
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len().max(1) as f32).sqrt()
}

fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_owned(),
        (_, true) => a.to_owned(),
        _ => format!("{a} {b}"),
    }
}

/// Per-frame RMS and the loudest one.
fn levels(pcm: &[f32]) -> (Vec<f32>, f32) {
    let v: Vec<f32> = pcm.chunks(FRAME).map(rms).collect();
    let peak = v.iter().copied().fold(0.0, f32::max);
    (v, peak)
}

/// Where to cut `pcm` after a finished phrase: just past the speech that precedes the first pause
/// of [`PAUSE_FRAMES`] quiet frames, or at the quietest point once `pcm` exceeds [`MAX_PENDING`].
fn cut_point(pcm: &[f32]) -> Option<usize> {
    let (lv, peak) = levels(pcm);
    if peak < AUDIBLE_RMS {
        return None;
    }
    let thr = (peak * 0.1).max(AUDIBLE_RMS * 0.6);
    let (mut spoke, mut quiet_from) = (false, None::<usize>);
    for (i, l) in lv.iter().enumerate() {
        if *l >= thr {
            spoke = true;
            quiet_from = None;
        } else if spoke {
            let start = *quiet_from.get_or_insert(i);
            if i + 1 - start >= PAUSE_FRAMES {
                return Some(((start + KEEP_FRAMES) * FRAME).min(pcm.len()));
            }
        }
    }
    if pcm.len() > MAX_PENDING {
        return crate::chunking::split_at_pauses(pcm, MAX_PENDING)
            .first()
            .map(|p| p.len());
    }
    None
}

/// The live utterance being transcribed.
#[derive(Debug, Default)]
pub struct LiveStream {
    id: Option<u64>,
    committed: String,
    words: Vec<Word>,
    pending: Vec<f32>,
    /// Seconds of audio already committed (word timestamps are shifted by this).
    offset_s: f32,
    opts: SttOpts,
    lang: String,
    /// Loudest frame of the utterance so far: audio far below it is a decayed word tail, not speech.
    peak: f32,
}

impl LiveStream {
    fn floor(&self) -> f32 {
        AUDIBLE_RMS.max(self.peak * 0.12)
    }

    fn decode(&mut self, stt: &mut dyn Stt, pcm: &[f32]) -> Result<Transcript, SttError> {
        if levels(pcm).1 < self.floor() {
            return Ok(Transcript::default());
        }
        stt.transcribe(pcm, &self.opts)
    }

    fn commit(&mut self, t: Transcript, samples: usize) {
        self.committed = join(&self.committed, &t.text);
        if self.lang.is_empty() {
            self.lang = t.lang;
        }
        let off = self.offset_s;
        self.words.extend(t.words.into_iter().map(|mut w| {
            w.start += off;
            w.end += off;
            w
        }));
        self.offset_s += samples as f32 / STT_SAMPLE_RATE as f32;
    }

    /// Drops the silence at the start of the pending audio, keeping a short lead-in.
    fn trim_lead(&mut self) {
        let (lv, peak) = levels(&self.pending);
        let thr = (peak * 0.1).max(AUDIBLE_RMS * 0.6);
        let first = lv.iter().position(|l| *l >= thr).unwrap_or(lv.len());
        let drop = first.saturating_sub(KEEP_FRAMES) * FRAME;
        self.pending.drain(..drop.min(self.pending.len()));
        self.offset_s += drop as f32 / STT_SAMPLE_RATE as f32;
    }

    /// Feeds audio of stream `id`; returns the caption so far (committed phrases + the live one).
    pub fn chunk(
        &mut self,
        id: u64,
        pcm: &[f32],
        opts: SttOpts,
        stt: &mut dyn Stt,
    ) -> Result<String, SttError> {
        if self.id != Some(id) {
            *self = Self {
                id: Some(id),
                ..Self::default()
            };
        }
        self.opts = opts;
        self.peak = self.peak.max(levels(pcm).1);
        self.pending.extend_from_slice(pcm);
        while let Some(cut) = cut_point(&self.pending) {
            let head: Vec<f32> = self.pending.drain(..cut).collect();
            let t = self.decode(stt, &head)?;
            self.commit(t, head.len());
            self.trim_lead();
        }
        let peak = levels(&self.pending).1;
        if peak < self.floor() {
            // Only silence so far: do not let it grow, keep a little lead-in.
            let keep = FRAME * KEEP_FRAMES;
            if self.pending.len() > keep {
                let drop = self.pending.len() - keep;
                self.pending.drain(..drop);
                self.offset_s += drop as f32 / STT_SAMPLE_RATE as f32;
            }
            return Ok(self.committed.clone());
        }
        let pending = std::mem::take(&mut self.pending);
        let live = self.decode(stt, &pending);
        self.pending = pending;
        Ok(join(&self.committed, &live?.text))
    }

    /// Ends stream `id`: decodes what is left and returns the whole utterance. A stream that
    /// never received audio (or another id) yields an empty transcript.
    pub fn stop(&mut self, id: u64, stt: &mut dyn Stt) -> Result<Transcript, SttError> {
        if self.id != Some(id) {
            *self = Self::default();
            return Ok(Transcript::default());
        }
        let mut done = std::mem::take(self);
        let pending = std::mem::take(&mut done.pending);
        let tail = done.decode(stt, &pending)?;
        done.commit(tail, pending.len());
        Ok(Transcript {
            text: done.committed,
            lang: done.lang,
            words: done.words,
            ttft_ms: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts the audio each call decodes and reports its length as the text.
    #[derive(Default)]
    struct Probe(Vec<usize>);
    impl Stt for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }
        fn transcribe(&mut self, pcm: &[f32], _: &SttOpts) -> Result<Transcript, SttError> {
            self.0.push(pcm.len());
            Ok(Transcript {
                text: format!("p{}", self.0.len()),
                lang: "es".into(),
                ..Default::default()
            })
        }
    }

    fn speech(secs: f32) -> Vec<f32> {
        (0..(secs * 16_000.0) as usize)
            .map(|i| (i as f32 * 0.07).sin() * 0.3)
            .collect()
    }
    fn silence(secs: f32) -> Vec<f32> {
        vec![0.0; (secs * 16_000.0) as usize]
    }

    #[test]
    fn a_finished_phrase_is_committed_and_the_tail_is_all_that_is_left_at_stop() {
        let mut live = LiveStream::default();
        let mut stt = Probe::default();
        let mut a = speech(1.0);
        a.extend(silence(0.5));
        a.extend(speech(0.5));
        let caption = live
            .chunk(1, &a, SttOpts::default(), &mut stt)
            .expect("chunk");
        // phrase 1 committed (call 1), the live phrase decoded for the caption (call 2)
        assert_eq!(caption, "p1 p2");
        assert_eq!(stt.0.len(), 2);
        let t = live.stop(1, &mut stt).expect("stop");
        assert_eq!(t.text, "p1 p3");
        assert_eq!(t.lang, "es");
        // the last decode saw only the second phrase, not the first
        assert!(stt.0[2] < 16_000 * 3 / 4, "{:?}", stt.0);
    }

    #[test]
    fn silence_is_never_sent_to_the_engine() {
        let mut live = LiveStream::default();
        let mut stt = Probe::default();
        for _ in 0..10 {
            live.chunk(1, &silence(0.75), SttOpts::default(), &mut stt)
                .expect("chunk");
        }
        assert!(live.stop(1, &mut stt).expect("stop").text.is_empty());
        assert!(stt.0.is_empty());
    }

    #[test]
    fn long_speech_without_pauses_is_cut_so_pending_stays_bounded() {
        let mut live = LiveStream::default();
        let mut stt = Probe::default();
        let s = speech(12.0);
        for c in s.chunks(12_000) {
            live.chunk(1, c, SttOpts::default(), &mut stt)
                .expect("chunk");
        }
        assert!(
            stt.0.iter().all(|n| *n <= MAX_PENDING + 12_000),
            "{:?}",
            stt.0
        );
    }

    #[test]
    fn another_stream_id_starts_afresh() {
        let mut live = LiveStream::default();
        let mut stt = Probe::default();
        live.chunk(1, &speech(1.0), SttOpts::default(), &mut stt)
            .expect("chunk");
        assert!(live.stop(2, &mut stt).expect("stop").text.is_empty());
    }
}
