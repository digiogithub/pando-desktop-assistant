//! The worker protocol loop: reads [`ToWorker`] frames, drives an [`Stt`] engine, writes
//! [`FromWorker`] frames. Single-threaded because the engines are not thread-safe.

use std::io::{Read, Write};
use std::path::Path;

use voice_proto::{
    ErrorCode, FromWorker, PROTOCOL_VERSION, ProtoError, Role, ToWorker, check_version, read_msg,
    write_msg,
};

use crate::handsfree::{HandsFree, HfEvent};
use crate::stt::{Stt, SttError, SttOpts};
use crate::vad::{EnergySegmenter, SpeechSegmenter};
use crate::wake::{SttWakeVerifier, WakeConfig};

/// Builds an engine from a model path.
pub type Loader<'a> = dyn FnMut(&Path) -> Result<Box<dyn Stt>, SttError> + 'a;

fn code_for(e: &SttError) -> ErrorCode {
    match e {
        SttError::Load(_) | SttError::AlreadyLoaded => ErrorCode::ModelLoad,
        SttError::Invalid(_) | SttError::Unsupported(_) => ErrorCode::BadRequest,
        SttError::Engine(_) | SttError::Json(_) => ErrorCode::Engine,
    }
}

fn err_msg(id: Option<u64>, e: &SttError) -> FromWorker {
    FromWorker::Error {
        id,
        code: code_for(e),
        message: e.to_string(),
    }
}

fn opts(lang: Option<String>, keywords: Vec<String>, word_ts: bool) -> SttOpts {
    SttOpts {
        lang,
        keywords,
        word_ts,
    }
}

/// Serve requests until `Shutdown`, end of input, or a fatal protocol error.
pub fn serve<R: Read, W: Write>(
    r: &mut R,
    w: &mut W,
    load: &mut Loader<'_>,
) -> Result<(), ProtoError> {
    let mut engine: Option<Box<dyn Stt>> = None;
    let mut hands_free: Option<HandsFree> = None;
    // The engine reports the text committed by each call only, so a live stream is accumulated here.
    let mut stream = StreamText::default();
    while let Some(msg) = read_msg::<_, ToWorker>(r)? {
        // Hands-free control and audio can produce several replies per message.
        let msg = match msg {
            ToWorker::HandsFree {
                enabled,
                lang,
                keywords,
                sensitivity,
                vad_model,
            } => {
                hands_free = None;
                if enabled {
                    let wake = WakeConfig::from_sensitivity(sensitivity);
                    let segmenter = make_segmenter(vad_model.as_deref());
                    let mut opts = SttOpts {
                        lang: lang.clone(),
                        keywords: keywords.clone(),
                        word_ts: false,
                    };
                    opts.keywords
                        .extend(crate::wake::WAKE_BIAS.iter().map(|k| k.to_string()));
                    hands_free = Some(HandsFree::new(
                        segmenter,
                        Box::new(SttWakeVerifier::new(wake.clone(), lang, &keywords)),
                        wake,
                        opts,
                    ));
                }
                continue;
            }
            ToWorker::Audio { pcm } => {
                let (Some(hf), Some(stt)) = (hands_free.as_mut(), engine.as_mut()) else {
                    continue; // hands-free off or no model: drop the audio silently
                };
                match hf.push(&pcm, stt.as_mut()) {
                    Ok(events) => {
                        for e in events {
                            write_msg(w, &hf_message(e))?;
                        }
                    }
                    Err(e) => write_msg(w, &err_msg(None, &e))?,
                }
                continue;
            }
            other => other,
        };
        let reply = match msg {
            ToWorker::Hello { version } => match check_version(version) {
                Ok(()) => Some(FromWorker::Hello {
                    version: PROTOCOL_VERSION,
                    role: Role::Stt,
                }),
                Err((code, message)) => {
                    write_msg(
                        w,
                        &FromWorker::Error {
                            id: None,
                            code,
                            message,
                        },
                    )?;
                    return Ok(());
                }
            },
            ToWorker::LoadStt { model_path } => {
                engine = None; // release the process-global model first
                match load(Path::new(&model_path)) {
                    Ok(e) => {
                        engine = Some(e);
                        Some(FromWorker::Ready)
                    }
                    Err(e) => Some(err_msg(None, &e)),
                }
            }
            ToWorker::Transcribe {
                id,
                pcm,
                lang,
                keywords,
                word_ts,
            } => Some(match engine.as_mut() {
                None => not_loaded(Some(id)),
                Some(e) => match e.transcribe(&pcm, &opts(lang, keywords, word_ts)) {
                    Ok(t) => FromWorker::Transcript {
                        id,
                        text: t.text,
                        lang: t.lang,
                        words: t.words,
                        ttft_ms: t.ttft_ms,
                    },
                    Err(e) => err_msg(Some(id), &e),
                },
            }),
            ToWorker::StreamChunk {
                id,
                pcm,
                lang,
                keywords,
            } => Some(match engine.as_mut() {
                None => not_loaded(Some(id)),
                Some(e) => match e.stream_process(&pcm, &opts(lang, keywords, false)) {
                    Ok(p) => FromWorker::Partial {
                        id,
                        text: stream.chunk(id, p),
                    },
                    Err(e) => err_msg(Some(id), &e),
                },
            }),
            ToWorker::StreamStop { id } => Some(match engine.as_mut() {
                None => not_loaded(Some(id)),
                Some(e) => {
                    let result = e.stream_stop();
                    let acc = stream.take(id);
                    match result {
                        Ok(t) => {
                            let (text, words) = acc.finish(t.text, t.words);
                            FromWorker::Transcript {
                                id,
                                text,
                                lang: t.lang,
                                words,
                                ttft_ms: t.ttft_ms,
                            }
                        }
                        Err(e) => err_msg(Some(id), &e),
                    }
                }
            }),
            // Calls are synchronous: by the time a Cancel is read the work is already done.
            ToWorker::Cancel { .. } => None,
            ToWorker::Health => Some(FromWorker::Health {
                role: Role::Stt,
                loaded: engine.is_some(),
                rss_bytes: None,
            }),
            ToWorker::Shutdown => return Ok(()),
            ToWorker::HandsFree { .. } | ToWorker::Audio { .. } => None, // handled above
            ToWorker::LoadTts { .. } | ToWorker::Synth { .. } => Some(FromWorker::Error {
                id: None,
                code: ErrorCode::BadRequest,
                message: "this is the STT worker".into(),
            }),
        };
        if let Some(m) = reply {
            write_msg(w, &m)?;
        }
    }
    Ok(())
}

/// Text and words committed so far by one live stream.
#[derive(Default)]
struct StreamText {
    id: Option<u64>,
    text: String,
    words: Vec<voice_proto::Word>,
}

impl StreamText {
    fn join(a: &str, b: &str) -> String {
        match (a.is_empty(), b.is_empty()) {
            (true, _) => b.to_owned(),
            (_, true) => a.to_owned(),
            _ => format!("{a} {b}"),
        }
    }

    /// Adds one `stream_process` result and returns the caption so far (committed + pending).
    fn chunk(&mut self, id: u64, p: crate::stt::Partial) -> String {
        if self.id != Some(id) {
            *self = Self {
                id: Some(id),
                ..Self::default()
            };
        }
        self.text = Self::join(&self.text, &p.text);
        self.words.extend(p.words);
        Self::join(&self.text, &p.pending)
    }

    /// The accumulated state of stream `id` (empty for another or no stream), reset for the next.
    fn take(&mut self, id: u64) -> Self {
        let acc = std::mem::take(self);
        if acc.id == Some(id) {
            acc
        } else {
            Self::default()
        }
    }

    /// Joins the committed text with the engine's tail.
    fn finish(
        self,
        tail: String,
        tail_words: Vec<voice_proto::Word>,
    ) -> (String, Vec<voice_proto::Word>) {
        let mut words = self.words;
        words.extend(tail_words);
        (Self::join(&self.text, &tail), words)
    }
}

fn make_segmenter(vad_model: Option<&str>) -> Box<dyn SpeechSegmenter> {
    #[cfg(feature = "silero-vad")]
    if let Some(m) = vad_model {
        match crate::vad::SileroSegmenter::new(std::path::Path::new(m)) {
            Ok(s) => return Box::new(s),
            Err(e) => eprintln!("pando-stt: {e}; using the energy VAD"),
        }
    }
    #[cfg(not(feature = "silero-vad"))]
    if vad_model.is_some() {
        eprintln!("pando-stt: built without the silero-vad feature; using the energy VAD");
    }
    Box::new(EnergySegmenter::default())
}

fn hf_message(e: HfEvent) -> FromWorker {
    match e {
        HfEvent::SpeechStart => FromWorker::SpeechStart,
        HfEvent::SpeechEnd => FromWorker::SpeechEnd,
        HfEvent::Wake { score } => FromWorker::WakeWord { score },
        HfEvent::Utterance { text, lang } => FromWorker::Utterance { text, lang },
    }
}

fn not_loaded(id: Option<u64>) -> FromWorker {
    FromWorker::Error {
        id,
        code: ErrorCode::NotLoaded,
        message: "no speech model loaded".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::Transcript;
    use std::io::Cursor;

    #[derive(Debug)]
    struct Fake;
    impl Stt for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn transcribe(&mut self, pcm: &[f32], o: &SttOpts) -> Result<Transcript, SttError> {
            Ok(Transcript {
                text: format!("{} samples, kw={}", pcm.len(), o.keywords.len()),
                lang: o.lang.clone().unwrap_or_default(),
                ..Default::default()
            })
        }
    }

    /// Streams like Needle: each call commits some words, `stop` returns only the tail.
    #[derive(Debug, Default)]
    struct Streamer(usize);
    impl Stt for Streamer {
        fn name(&self) -> &'static str {
            "streamer"
        }
        fn transcribe(&mut self, _: &[f32], _: &SttOpts) -> Result<Transcript, SttError> {
            Ok(Transcript::default())
        }
        fn stream_process(
            &mut self,
            _: &[f32],
            _: &SttOpts,
        ) -> Result<crate::stt::Partial, SttError> {
            self.0 += 1;
            Ok(crate::stt::Partial {
                text: ["Oye Pando,", "", "abre el proyecto"][(self.0 - 1).min(2)].into(),
                pending: "bita".into(),
                words: Vec::new(),
            })
        }
        fn stream_stop(&mut self) -> Result<Transcript, SttError> {
            self.0 = 0;
            Ok(Transcript {
                text: "Bitácora.".into(),
                ..Default::default()
            })
        }
    }

    #[test]
    fn a_stream_returns_all_committed_text_not_only_the_tail() {
        let chunk = |id| ToWorker::StreamChunk {
            id,
            pcm: vec![0.0; 16_000],
            lang: None,
            keywords: Vec::new(),
        };
        let mut input = Vec::new();
        for m in [
            chunk(1),
            chunk(1),
            chunk(1),
            ToWorker::StreamStop { id: 1 },
            ToWorker::StreamStop { id: 2 },
        ] {
            write_msg(&mut input, &m).expect("write");
        }
        let mut out = Vec::new();
        let mut load =
            |_: &Path| -> Result<Box<dyn Stt>, SttError> { Ok(Box::new(Streamer::default())) };
        let mut all = Vec::new();
        write_msg(
            &mut all,
            &ToWorker::LoadStt {
                model_path: "x".into(),
            },
        )
        .expect("write");
        all.extend(input);
        serve(&mut Cursor::new(all), &mut out, &mut load).expect("serve");
        let mut cur = Cursor::new(out);
        let mut v = Vec::new();
        while let Some(m) = read_msg::<_, FromWorker>(&mut cur).expect("read") {
            v.push(m);
        }
        let partials: Vec<_> = v
            .iter()
            .filter_map(|m| match m {
                FromWorker::Partial { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            partials,
            [
                "Oye Pando, bita",
                "Oye Pando, bita",
                "Oye Pando, abre el proyecto bita"
            ]
        );
        let finals: Vec<_> = v
            .iter()
            .filter_map(|m| match m {
                FromWorker::Transcript { id, text, .. } => Some((*id, text.as_str())),
                _ => None,
            })
            .collect();
        // Stream 2 never received a chunk, so only the engine's tail is its text.
        assert_eq!(
            finals,
            [
                (1, "Oye Pando, abre el proyecto Bitácora."),
                (2, "Bitácora.")
            ]
        );
    }

    fn run(msgs: &[ToWorker]) -> Vec<FromWorker> {
        let mut input = Vec::new();
        for m in msgs {
            write_msg(&mut input, m).expect("write");
        }
        let mut out = Vec::new();
        let mut load = |_: &Path| -> Result<Box<dyn Stt>, SttError> { Ok(Box::new(Fake)) };
        serve(&mut Cursor::new(input), &mut out, &mut load).expect("serve");
        let mut cur = Cursor::new(out);
        let mut v = Vec::new();
        while let Some(m) = read_msg::<_, FromWorker>(&mut cur).expect("read") {
            v.push(m);
        }
        v
    }

    #[test]
    fn handshake_load_transcribe() {
        let out = run(&[
            ToWorker::Hello {
                version: PROTOCOL_VERSION,
            },
            ToWorker::LoadStt {
                model_path: "m".into(),
            },
            ToWorker::Transcribe {
                id: 1,
                pcm: vec![0.0; 10],
                lang: Some("es".into()),
                keywords: vec!["Pando".into()],
                word_ts: false,
            },
            ToWorker::Health,
            ToWorker::Shutdown,
        ]);
        assert!(matches!(
            out[0],
            FromWorker::Hello {
                role: Role::Stt,
                ..
            }
        ));
        assert_eq!(out[1], FromWorker::Ready);
        assert!(
            matches!(&out[2], FromWorker::Transcript { id: 1, text, lang, .. }
            if text == "10 samples, kw=1" && lang == "es")
        );
        assert!(matches!(out[3], FromWorker::Health { loaded: true, .. }));
    }

    #[test]
    fn hands_free_over_the_protocol() {
        let mut seed = 77u32;
        let mut noise = |n: usize, amp: f32| -> Vec<f32> {
            (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp
                })
                .collect()
        };
        let mut msgs = vec![
            ToWorker::Hello {
                version: PROTOCOL_VERSION,
            },
            ToWorker::LoadStt {
                model_path: "m".into(),
            },
            ToWorker::HandsFree {
                enabled: true,
                lang: Some("es".into()),
                keywords: vec![],
                sensitivity: 0.4,
                vad_model: None,
            },
        ];
        for block in [
            noise(32_000, 0.002),
            noise(24_000, 0.3),
            noise(40_000, 0.002),
        ] {
            for c in block.chunks(4000) {
                msgs.push(ToWorker::Audio { pcm: c.to_vec() });
            }
        }
        msgs.push(ToWorker::HandsFree {
            enabled: false,
            lang: None,
            keywords: vec![],
            sensitivity: 0.4,
            vad_model: None,
        });
        let out = run(&msgs);
        assert!(out.contains(&FromWorker::SpeechStart));
        assert!(out.contains(&FromWorker::SpeechEnd));
        // Fake engine says "N samples, kw=K": no wake phrase, so no WakeWord / Utterance.
        assert!(!out.iter().any(|m| matches!(
            m,
            FromWorker::WakeWord { .. } | FromWorker::Utterance { .. }
        )));
    }

    #[test]
    fn transcribe_before_load_is_an_error() {
        let out = run(&[ToWorker::Transcribe {
            id: 5,
            pcm: vec![],
            lang: None,
            keywords: vec![],
            word_ts: false,
        }]);
        assert!(matches!(
            out[0],
            FromWorker::Error {
                id: Some(5),
                code: ErrorCode::NotLoaded,
                ..
            }
        ));
    }

    #[test]
    fn version_mismatch_stops_the_session() {
        let out = run(&[ToWorker::Hello { version: 999 }, ToWorker::Health]);
        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            FromWorker::Error {
                code: ErrorCode::VersionMismatch,
                ..
            }
        ));
    }

    #[test]
    fn tts_requests_are_rejected() {
        let out = run(&[ToWorker::LoadTts {
            model_dir: "d".into(),
            voice: "v".into(),
        }]);
        assert!(matches!(
            out[0],
            FromWorker::Error {
                code: ErrorCode::BadRequest,
                ..
            }
        ));
    }
}
