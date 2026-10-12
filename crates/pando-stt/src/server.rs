//! The worker protocol loop: reads [`ToWorker`] frames, drives an [`Stt`] engine, writes
//! [`FromWorker`] frames. Single-threaded because the engines are not thread-safe.

use std::io::{Read, Write};
use std::path::Path;

use voice_proto::{
    ErrorCode, FromWorker, PROTOCOL_VERSION, ProtoError, Role, ToWorker, check_version, read_msg,
    write_msg,
};

use crate::stt::{Stt, SttError, SttOpts};

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
    while let Some(msg) = read_msg::<_, ToWorker>(r)? {
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
                    Ok(p) => FromWorker::Partial { id, text: p.text },
                    Err(e) => err_msg(Some(id), &e),
                },
            }),
            ToWorker::StreamStop { id } => Some(match engine.as_mut() {
                None => not_loaded(Some(id)),
                Some(e) => match e.stream_stop() {
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
            // Calls are synchronous: by the time a Cancel is read the work is already done.
            ToWorker::Cancel { .. } => None,
            ToWorker::Health => Some(FromWorker::Health {
                role: Role::Stt,
                loaded: engine.is_some(),
                rss_bytes: None,
            }),
            ToWorker::Shutdown => return Ok(()),
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
