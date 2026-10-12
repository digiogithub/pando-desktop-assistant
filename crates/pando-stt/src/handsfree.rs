//! Hands-free pipeline (PDA-T-0136 + PDA-T-0137): continuous audio -> VAD segments -> wake
//! verification -> command. Audio is only kept in memory while a segment is open; the STT is never
//! invoked during silence. Nothing is written to disk.

use crate::stt::{Stt, SttError, SttOpts};
use crate::vad::{Segment, SpeechSegmenter, VadEvent};
use crate::wake::{VERIFY_SECONDS, WakeConfig, WakeDetector, match_wake};

/// What the pipeline reports.
#[derive(Debug, Clone, PartialEq)]
pub enum HfEvent {
    SpeechStart,
    SpeechEnd,
    /// Wake phrase accepted.
    Wake {
        score: f32,
    },
    /// Command text (wake phrase removed).
    Utterance {
        text: String,
        lang: String,
    },
}

/// Segments shorter than this are not worth a verification pass.
const MIN_VERIFY_SECONDS: f32 = 0.4;
/// After a bare "Oye Pando" the next segment within this time is the command.
const COMMAND_WINDOW_SAMPLES: u64 = 8 * 16_000;

/// The hands-free state machine.
pub struct HandsFree {
    segmenter: Box<dyn SpeechSegmenter>,
    detector: Box<dyn WakeDetector>,
    wake: WakeConfig,
    opts: SttOpts,
    seen: u64,
    awaiting_until: Option<u64>,
    /// Number of verification passes run (for tests and CPU accounting).
    pub verifications: u64,
}

impl std::fmt::Debug for HandsFree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HandsFree")
    }
}

impl HandsFree {
    /// `opts`: language and keyword bias for the command pass.
    pub fn new(
        segmenter: Box<dyn SpeechSegmenter>,
        detector: Box<dyn WakeDetector>,
        wake: WakeConfig,
        opts: SttOpts,
    ) -> Self {
        Self {
            segmenter,
            detector,
            wake,
            opts,
            seen: 0,
            awaiting_until: None,
            verifications: 0,
        }
    }

    /// Feed audio. `stt` is only used when a segment closes.
    pub fn push(&mut self, pcm: &[f32], stt: &mut dyn Stt) -> Result<Vec<HfEvent>, SttError> {
        let mut vad = Vec::new();
        self.segmenter.push(pcm, &mut vad);
        self.seen += pcm.len() as u64;
        self.handle(vad, stt)
    }

    /// Stop: close an open segment.
    pub fn finish(&mut self, stt: &mut dyn Stt) -> Result<Vec<HfEvent>, SttError> {
        let mut vad = Vec::new();
        self.segmenter.flush(&mut vad);
        self.handle(vad, stt)
    }

    fn handle(&mut self, vad: Vec<VadEvent>, stt: &mut dyn Stt) -> Result<Vec<HfEvent>, SttError> {
        let mut out = Vec::new();
        for ev in vad {
            match ev {
                VadEvent::Start => out.push(HfEvent::SpeechStart),
                VadEvent::End(seg) => {
                    out.push(HfEvent::SpeechEnd);
                    self.segment(seg, stt, &mut out)?;
                }
            }
        }
        Ok(out)
    }

    fn segment(
        &mut self,
        seg: Segment,
        stt: &mut dyn Stt,
        out: &mut Vec<HfEvent>,
    ) -> Result<(), SttError> {
        if let Some(deadline) = self.awaiting_until.take()
            && self.seen <= deadline + seg.samples.len() as u64
        {
            // the command that follows a bare wake phrase
            let t = stt.transcribe(&seg.samples, &self.opts)?;
            if !t.text.is_empty() {
                out.push(HfEvent::Utterance {
                    text: t.text,
                    lang: t.lang,
                });
            }
            return Ok(());
        }
        if seg.seconds() < MIN_VERIFY_SECONDS {
            return Ok(());
        }
        self.verifications += 1;
        let Some(hit) = self.detector.check(&seg.samples, stt)? else {
            return Ok(());
        };
        out.push(HfEvent::Wake { score: hit.score });
        let mut rest = hit.rest;
        if seg.seconds() > VERIFY_SECONDS {
            // the command continues beyond the verification window: transcribe it all
            let full = stt.transcribe(&seg.samples, &self.opts)?;
            rest = match_wake(&full.text, &self.wake).map_or(full.text, |m| m.rest);
        }
        if rest.is_empty() {
            self.awaiting_until = Some(self.seen + COMMAND_WINDOW_SAMPLES);
        } else {
            out.push(HfEvent::Utterance {
                text: rest,
                lang: self.opts.lang.clone().unwrap_or_default(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::Transcript;
    use crate::vad::{EnergySegmenter, testutil::noise};
    use crate::wake::SttWakeVerifier;

    /// Returns scripted transcripts in order and counts calls.
    struct Script {
        texts: Vec<&'static str>,
        calls: usize,
    }
    impl Stt for Script {
        fn name(&self) -> &'static str {
            "script"
        }
        fn transcribe(&mut self, _pcm: &[f32], _o: &SttOpts) -> Result<Transcript, SttError> {
            let t = self.texts.get(self.calls).copied().unwrap_or("");
            self.calls += 1;
            Ok(Transcript {
                text: t.into(),
                lang: "es".into(),
                ..Default::default()
            })
        }
    }

    fn pipeline() -> HandsFree {
        HandsFree::new(
            Box::new(EnergySegmenter::default()),
            Box::new(SttWakeVerifier::new(
                WakeConfig::default(),
                Some("es".into()),
                &[],
            )),
            WakeConfig::default(),
            SttOpts {
                lang: Some("es".into()),
                ..Default::default()
            },
        )
    }

    fn feed(h: &mut HandsFree, stt: &mut Script, utterances: &[f32]) -> Vec<HfEvent> {
        let mut seed = 99;
        let mut ev = Vec::new();
        let mut pcm = noise(16_000 * 2, 0.002, &mut seed);
        for secs in utterances {
            pcm.extend(noise((16_000.0 * secs) as usize, 0.3, &mut seed));
            pcm.extend(noise(16_000 * 2, 0.002, &mut seed));
        }
        for c in pcm.chunks(1600) {
            ev.extend(h.push(c, stt).expect("push"));
        }
        ev.extend(h.finish(stt).expect("finish"));
        ev
    }

    #[test]
    fn silence_never_invokes_the_stt() {
        let mut h = pipeline();
        let mut stt = Script {
            texts: vec![],
            calls: 0,
        };
        let mut seed = 5;
        for _ in 0..120 {
            h.push(&noise(16_000, 0.002, &mut seed), &mut stt)
                .expect("push");
        }
        assert_eq!(stt.calls, 0);
        assert_eq!(h.verifications, 0);
    }

    #[test]
    fn short_wake_with_command_in_one_utterance() {
        let mut h = pipeline();
        let mut stt = Script {
            texts: vec!["Oye Pando, cancela migración auth"],
            calls: 0,
        };
        let ev = feed(&mut h, &mut stt, &[1.2]);
        assert_eq!(ev[0], HfEvent::SpeechStart);
        assert!(ev.contains(&HfEvent::Wake { score: 1.0 }));
        assert!(ev.contains(&HfEvent::Utterance {
            text: "cancela migración auth".into(),
            lang: "es".into()
        }));
        assert_eq!(stt.calls, 1, "one verification pass only");
    }

    #[test]
    fn unrelated_speech_is_ignored() {
        let mut h = pipeline();
        let mut stt = Script {
            texts: vec!["hola qué tal estás"],
            calls: 0,
        };
        let ev = feed(&mut h, &mut stt, &[1.5]);
        assert!(
            !ev.iter()
                .any(|e| matches!(e, HfEvent::Wake { .. } | HfEvent::Utterance { .. }))
        );
        assert!(ev.contains(&HfEvent::SpeechEnd));
    }

    #[test]
    fn bare_wake_then_command_in_the_next_segment() {
        let mut h = pipeline();
        let mut stt = Script {
            texts: vec!["Oye Pando.", "qué tareas hay pendientes"],
            calls: 0,
        };
        let ev = feed(&mut h, &mut stt, &[1.2, 2.0]);
        let utt: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e, HfEvent::Utterance { .. }))
            .collect();
        assert_eq!(utt.len(), 1);
        assert!(
            matches!(utt[0], HfEvent::Utterance { text, .. } if text == "qué tareas hay pendientes")
        );
    }

    #[test]
    fn long_utterance_gets_a_second_full_pass() {
        let mut h = pipeline();
        let mut stt = Script {
            texts: vec![
                "Oye Pando, cancela",
                "Oye Pando, cancela la migración de autenticación por favor",
            ],
            calls: 0,
        };
        let ev = feed(&mut h, &mut stt, &[4.0]);
        assert!(ev.contains(&HfEvent::Utterance {
            text: "cancela la migración de autenticación por favor".into(),
            lang: "es".into()
        }));
        assert_eq!(stt.calls, 2);
    }

    #[test]
    fn very_short_blips_are_not_verified() {
        let mut h = pipeline();
        let mut stt = Script {
            texts: vec!["Oye Pando"],
            calls: 0,
        };
        let ev = feed(&mut h, &mut stt, &[0.15]);
        // 0.3 s of noise is dropped by the VAD (min speech) or skipped as too short
        assert!(!ev.iter().any(|e| matches!(e, HfEvent::Wake { .. })));
    }
}
