//! `voice-proto` loop of the TTS worker. A reader thread (this one) takes requests and handles
//! `Cancel` immediately; a synthesis thread owns the engine and works through load/synth jobs in
//! order, so cancelling never waits behind a running synthesis.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use voice_proto::{
    ErrorCode, FromWorker, PROTOCOL_VERSION, ProtoError, Role, ToWorker, check_version, read_msg,
    write_msg,
};

use crate::engine::{Tts, TtsError, synthesize_stream};

/// Builds an engine from `(model_dir, voice)`.
pub type Loader = dyn FnMut(&std::path::Path, &str) -> Result<Box<dyn Tts>, TtsError> + Send;

enum Job {
    Load { dir: PathBuf, voice: String },
    Synth { id: u64, text: String, speed: f32 },
}

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

fn send(out: &Out, msg: &FromWorker) {
    if let Ok(mut w) = out.lock() {
        let _ = write_msg(&mut *w, msg);
    }
}

fn err(id: Option<u64>, code: ErrorCode, message: String) -> FromWorker {
    FromWorker::Error { id, code, message }
}

struct Shared {
    cancelled: Mutex<HashSet<u64>>,
    /// Id of the synthesis that is running, and its abort flag.
    current: Mutex<Option<u64>>,
    abort: Arc<AtomicBool>,
}

fn run_jobs(rx: &Receiver<Job>, mut load: Box<Loader>, out: &Out, sh: &Shared) {
    let mut engine: Option<Box<dyn Tts>> = None;
    while let Ok(job) = rx.recv() {
        match job {
            Job::Load { dir, voice } => {
                engine = None; // free the old model before loading the next
                match load(&dir, &voice) {
                    Ok(e) => {
                        engine = Some(e);
                        send(out, &FromWorker::Ready);
                    }
                    Err(e) => send(out, &err(None, ErrorCode::ModelLoad, e.to_string())),
                }
            }
            Job::Synth { id, text, speed } => {
                let was_cancelled = sh
                    .cancelled
                    .lock()
                    .map(|mut c| c.remove(&id))
                    .unwrap_or(false);
                if was_cancelled {
                    send(
                        out,
                        &err(Some(id), ErrorCode::Cancelled, "cancelled".into()),
                    );
                    continue;
                }
                let Some(e) = engine.as_mut() else {
                    send(
                        out,
                        &err(Some(id), ErrorCode::NotLoaded, "no voice loaded".into()),
                    );
                    continue;
                };
                if let Ok(mut cur) = sh.current.lock() {
                    *cur = Some(id);
                }
                sh.abort.store(false, Ordering::SeqCst);
                let res =
                    synthesize_stream(e.as_mut(), &text, speed, &sh.abort, &mut |rate, pcm| {
                        send(
                            out,
                            &FromWorker::Pcm {
                                id,
                                sample_rate: rate,
                                samples: pcm.to_vec(),
                            },
                        );
                        true
                    });
                if let Ok(mut cur) = sh.current.lock() {
                    *cur = None;
                }
                match res {
                    Ok(true) => send(out, &FromWorker::SynthDone { id }),
                    Ok(false) => send(
                        out,
                        &err(Some(id), ErrorCode::Cancelled, "cancelled".into()),
                    ),
                    Err(e) => send(out, &err(Some(id), ErrorCode::Engine, e.to_string())),
                }
            }
        }
    }
}

/// Serve until `Shutdown` or end of input.
pub fn serve<R: Read>(
    r: &mut R,
    w: Box<dyn Write + Send>,
    load: Box<Loader>,
) -> Result<(), ProtoError> {
    let out: Out = Arc::new(Mutex::new(w));
    let sh = Arc::new(Shared {
        cancelled: Mutex::new(HashSet::new()),
        current: Mutex::new(None),
        abort: Arc::new(AtomicBool::new(false)),
    });
    let (tx, rx) = channel::<Job>();
    let (o2, s2) = (out.clone(), sh.clone());
    let worker = std::thread::Builder::new()
        .name("tts-synth".into())
        .spawn(move || run_jobs(&rx, load, &o2, &s2))
        .map_err(ProtoError::Io)?;
    let mut loaded = false;
    let result = (|| {
        while let Some(msg) = read_msg::<_, ToWorker>(r)? {
            match msg {
                ToWorker::Hello { version } => match check_version(version) {
                    Ok(()) => send(
                        &out,
                        &FromWorker::Hello {
                            version: PROTOCOL_VERSION,
                            role: Role::Tts,
                        },
                    ),
                    Err((code, message)) => {
                        send(&out, &err(None, code, message));
                        return Ok(());
                    }
                },
                ToWorker::LoadTts { model_dir, voice } => {
                    loaded = true;
                    let _ = tx.send(Job::Load {
                        dir: PathBuf::from(model_dir),
                        voice,
                    });
                }
                ToWorker::Synth {
                    id, text, speed, ..
                } => {
                    let _ = tx.send(Job::Synth {
                        id,
                        text,
                        speed: if speed > 0.0 { speed } else { 1.0 },
                    });
                }
                ToWorker::Cancel { id } => {
                    let running = sh.current.lock().map(|c| *c == Some(id)).unwrap_or(false);
                    if running {
                        sh.abort.store(true, Ordering::SeqCst);
                    } else if let Ok(mut c) = sh.cancelled.lock() {
                        c.insert(id);
                    }
                }
                ToWorker::Health => send(
                    &out,
                    &FromWorker::Health {
                        role: Role::Tts,
                        loaded,
                        rss_bytes: None,
                    },
                ),
                ToWorker::Shutdown => return Ok(()),
                ToWorker::LoadStt { .. }
                | ToWorker::Transcribe { .. }
                | ToWorker::StreamChunk { .. }
                | ToWorker::StreamStop { .. }
                | ToWorker::HandsFree { .. }
                | ToWorker::Audio { .. } => send(
                    &out,
                    &err(None, ErrorCode::BadRequest, "this is the TTS worker".into()),
                ),
            }
        }
        Ok(())
    })();
    sh.abort.store(true, Ordering::SeqCst);
    drop(tx);
    let _ = worker.join();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::AtomicBool;

    struct Fake {
        delay_ms: u64,
    }
    impl Tts for Fake {
        fn sample_rate(&self) -> u32 {
            100
        }
        fn synth_sentence(
            &mut self,
            text: &str,
            _s: f32,
            c: &Arc<AtomicBool>,
        ) -> Result<Vec<f32>, TtsError> {
            let t = std::time::Instant::now();
            while t.elapsed().as_millis() < u128::from(self.delay_ms) {
                if c.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Ok(vec![0.25; text.chars().count() * 10])
        }
    }

    /// Write sink shared with the test.
    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("lock").extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn run(msgs: &[ToWorker], delay_ms: u64) -> Vec<FromWorker> {
        let mut input = Vec::new();
        for m in msgs {
            write_msg(&mut input, m).expect("write");
        }
        let buf = Buf::default();
        let load: Box<Loader> = Box::new(move |_, _| Ok(Box::new(Fake { delay_ms })));
        serve(&mut Cursor::new(input), Box::new(buf.clone()), load).expect("serve");
        let bytes = buf.0.lock().expect("lock").clone();
        let mut cur = Cursor::new(bytes);
        let mut v = Vec::new();
        while let Some(m) = read_msg::<_, FromWorker>(&mut cur).expect("read") {
            v.push(m);
        }
        v
    }

    fn synth(id: u64, text: &str) -> ToWorker {
        ToWorker::Synth {
            id,
            text: text.into(),
            voice: String::new(),
            lang: None,
            speed: 1.0,
        }
    }

    #[test]
    fn load_synth_done() {
        // Shutdown is not sent: end of input lets the synthesis thread finish its queue.
        let out = run(
            &[
                ToWorker::Hello {
                    version: PROTOCOL_VERSION,
                },
                ToWorker::LoadTts {
                    model_dir: "m".into(),
                    voice: "v".into(),
                },
                synth(1, "Hola. Adiós."),
            ],
            0,
        );
        assert!(matches!(
            out[0],
            FromWorker::Hello {
                role: Role::Tts,
                ..
            }
        ));
        assert_eq!(out[1], FromWorker::Ready);
        let pcm: usize = out
            .iter()
            .filter_map(|m| match m {
                FromWorker::Pcm {
                    id: 1,
                    sample_rate: 100,
                    samples,
                } => Some(samples.len()),
                _ => None,
            })
            .sum();
        assert_eq!(pcm, (5 + 6) * 10);
        assert_eq!(out.last(), Some(&FromWorker::SynthDone { id: 1 }));
    }

    #[test]
    fn synth_before_load_is_not_loaded() {
        let out = run(&[synth(3, "Hola")], 0);
        assert!(matches!(
            out[0],
            FromWorker::Error {
                id: Some(3),
                code: ErrorCode::NotLoaded,
                ..
            }
        ));
    }

    #[test]
    fn cancel_before_start_skips_the_job() {
        let out = run(
            &[
                ToWorker::LoadTts {
                    model_dir: "m".into(),
                    voice: String::new(),
                },
                ToWorker::Cancel { id: 9 },
                synth(9, "Hola"),
            ],
            0,
        );
        assert!(out.iter().any(|m| matches!(
            m,
            FromWorker::Error {
                id: Some(9),
                code: ErrorCode::Cancelled,
                ..
            }
        )));
        assert!(!out.iter().any(|m| matches!(m, FromWorker::Pcm { .. })));
    }

    #[test]
    fn cancel_during_synthesis_aborts_it() {
        // Sentence 1 takes 150 ms; the Cancel arrives right after and must prevent sentence 2.
        let out = run(
            &[
                ToWorker::LoadTts {
                    model_dir: "m".into(),
                    voice: String::new(),
                },
                synth(5, "Uno. Dos. Tres."),
                ToWorker::Cancel { id: 5 },
            ],
            150,
        );
        // Either cancelled before it began (queue order) or aborted while running: never SynthDone.
        assert!(
            !out.iter()
                .any(|m| matches!(m, FromWorker::SynthDone { id: 5 }))
        );
        assert!(out.iter().any(|m| matches!(
            m,
            FromWorker::Error {
                id: Some(5),
                code: ErrorCode::Cancelled,
                ..
            }
        )));
    }

    #[test]
    fn stt_requests_are_rejected_and_version_checked() {
        let out = run(&[ToWorker::Hello { version: 999 }, ToWorker::Health], 0);
        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            FromWorker::Error {
                code: ErrorCode::VersionMismatch,
                ..
            }
        ));
        let out = run(
            &[ToWorker::LoadStt {
                model_path: "x".into(),
            }],
            0,
        );
        assert!(matches!(
            out[0],
            FromWorker::Error {
                code: ErrorCode::BadRequest,
                ..
            }
        ));
    }
}
