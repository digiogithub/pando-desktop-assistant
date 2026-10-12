//! Safe wrapper over the Whistle speech model in `libneedle.a`.
//!
//! The engine is process-global and not thread-safe: [`Whistle`] is `!Send + !Sync` and only
//! one can exist per process (atomic guard). Keep it on the worker's STT thread.

use std::ffi::{CStr, CString, c_char, c_int};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use voice_proto::Word;

use crate::chunking::split_at_pauses;
use crate::stt::{Partial, Stt, SttError, SttOpts, Transcript};

static LOADED: AtomicBool = AtomicBool::new(false);
/// Output buffer size, as in the reference wrapper.
const OUT_CAP: usize = 1 << 18;
/// Whistle's fast path ends at 5 s of audio (spike S1): never send more in one call.
pub const MAX_CHUNK_SECONDS: f32 = 5.0;

#[derive(Debug, Deserialize, Default)]
struct RawWord {
    word: String,
    #[serde(default)]
    start: f32,
    #[serde(default)]
    end: f32,
    #[serde(default)]
    probability: f32,
}

#[derive(Debug, Deserialize, Default)]
struct RawTranscript {
    #[serde(default)]
    text: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    ttft_ms: Option<f32>,
    #[serde(default)]
    words: Vec<RawWord>,
    #[serde(default)]
    pending: String,
}

fn words(raw: Vec<RawWord>) -> Vec<Word> {
    raw.into_iter()
        .map(|w| Word {
            word: w.word,
            start: w.start,
            end: w.end,
            probability: w.probability,
        })
        .collect()
}

/// Parse the engine's `transcribe` / `stream_stop` JSON.
pub fn parse_transcript(json: &str) -> Result<Transcript, SttError> {
    if json.trim().is_empty() {
        return Ok(Transcript::default());
    }
    let r: RawTranscript = serde_json::from_str(json).map_err(|e| SttError::Json(e.to_string()))?;
    Ok(Transcript {
        text: r.text.trim().to_string(),
        lang: r.language,
        words: words(r.words),
        ttft_ms: r.ttft_ms,
    })
}

/// Parse the engine's `stream_process` JSON.
pub fn parse_partial(json: &str) -> Result<Partial, SttError> {
    if json.trim().is_empty() {
        return Ok(Partial::default());
    }
    let r: RawTranscript = serde_json::from_str(json).map_err(|e| SttError::Json(e.to_string()))?;
    Ok(Partial {
        text: r.text.trim().to_string(),
        pending: r.pending,
        words: words(r.words),
    })
}

fn last_error() -> String {
    // SAFETY: the runtime returns a NUL-terminated string (or NULL) valid until the next call.
    unsafe {
        let p = needle_sys::needle_last_error();
        if p.is_null() {
            "unknown".into()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

fn opt_cstring(s: Option<&str>) -> Result<Option<CString>, SttError> {
    s.map(|v| CString::new(v).map_err(|e| SttError::Invalid(e.to_string())))
        .transpose()
}

fn ptr(s: &Option<CString>) -> *const c_char {
    s.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
}

/// Engine arguments derived from [`SttOpts`].
fn args(opts: &SttOpts) -> Result<(Option<CString>, Option<CString>), SttError> {
    let kw = crate::keywords::to_engine_string(&opts.keywords);
    Ok((
        opt_cstring(opts.lang.as_deref().filter(|l| !l.is_empty()))?,
        opt_cstring(kw.as_deref())?,
    ))
}

/// The Whistle speech model.
#[derive(Debug)]
pub struct Whistle {
    out: Vec<u8>,
    _not_send: PhantomData<*const ()>,
}

impl Whistle {
    /// Load a `.cact` speech model from memory (12-13 ms for the 17 MB Whistle).
    pub fn load(cact: &[u8]) -> Result<Self, SttError> {
        if LOADED.swap(true, Ordering::SeqCst) {
            return Err(SttError::AlreadyLoaded);
        }
        // SAFETY: pointer and length come from a live slice; the engine copies what it needs.
        let rc = unsafe { needle_sys::needle_load(cact.as_ptr(), cact.len() as u64) };
        if rc < 0 {
            LOADED.store(false, Ordering::SeqCst);
            return Err(SttError::Load(format!(
                "needle_load ({rc}): {}",
                last_error()
            )));
        }
        // SAFETY: plain query.
        if unsafe { needle_sys::needle_models() } & needle_sys::NEEDLE_SPEECH == 0 {
            LOADED.store(false, Ordering::SeqCst);
            return Err(SttError::Load("the .cact holds no speech model".into()));
        }
        Ok(Self {
            out: vec![0; OUT_CAP],
            _not_send: PhantomData,
        })
    }

    /// Load from a file path.
    pub fn load_file(path: &std::path::Path) -> Result<Self, SttError> {
        let bytes =
            std::fs::read(path).map_err(|e| SttError::Load(format!("{}: {e}", path.display())))?;
        Self::load(&bytes)
    }

    fn json(&self) -> String {
        let end = self
            .out
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.out.len());
        String::from_utf8_lossy(&self.out[..end]).into_owned()
    }

    /// One engine call, no chunking. Returns the raw JSON.
    fn transcribe_raw(&mut self, pcm: &[f32], opts: &SttOpts) -> Result<String, SttError> {
        let (lang, kw) = args(opts)?;
        self.out[0] = 0;
        // SAFETY: all buffers live for the call; `out` capacity equals the buffer length.
        let rc = unsafe {
            needle_sys::needle_transcribe(
                pcm.as_ptr(),
                pcm.len() as c_int,
                ptr(&lang),
                ptr(&kw),
                opts.word_ts as c_int,
                self.out.as_mut_ptr().cast(),
                OUT_CAP as c_int,
            )
        };
        if rc < 0 {
            return Err(SttError::Engine(format!(
                "needle_transcribe ({rc}): {}",
                last_error()
            )));
        }
        Ok(self.json())
    }
}

impl Drop for Whistle {
    fn drop(&mut self) {
        // SAFETY: drops the engine's per-stream state; the model stays loaded in the process.
        unsafe { needle_sys::needle_reset() };
        LOADED.store(false, Ordering::SeqCst);
    }
}

impl Stt for Whistle {
    fn name(&self) -> &'static str {
        "whistle"
    }

    fn preferred_max_seconds(&self) -> Option<f32> {
        Some(MAX_CHUNK_SECONDS)
    }

    /// Transcribes in pieces of at most 5 s (cut at pauses) and joins the results.
    fn transcribe(&mut self, pcm: &[f32], opts: &SttOpts) -> Result<Transcript, SttError> {
        let max = (MAX_CHUNK_SECONDS * voice_proto::STT_SAMPLE_RATE as f32) as usize;
        let mut acc = Transcript::default();
        let mut offset = 0.0f32;
        for piece in split_at_pauses(pcm, max) {
            let t = parse_transcript(&self.transcribe_raw(piece, opts)?)?;
            if !t.text.is_empty() {
                if !acc.text.is_empty() {
                    acc.text.push(' ');
                }
                acc.text.push_str(&t.text);
            }
            if acc.lang.is_empty() {
                acc.lang = t.lang;
            }
            acc.ttft_ms = acc.ttft_ms.or(t.ttft_ms);
            acc.words.extend(t.words.into_iter().map(|mut w| {
                w.start += offset;
                w.end += offset;
                w
            }));
            offset += piece.len() as f32 / voice_proto::STT_SAMPLE_RATE as f32;
        }
        Ok(acc)
    }

    fn stream_process(&mut self, pcm: &[f32], opts: &SttOpts) -> Result<Partial, SttError> {
        let (lang, kw) = args(opts)?;
        self.out[0] = 0;
        // SAFETY: as in `transcribe_raw`.
        let rc = unsafe {
            needle_sys::needle_stream_transcribe_process(
                pcm.as_ptr(),
                pcm.len() as c_int,
                ptr(&lang),
                ptr(&kw),
                self.out.as_mut_ptr().cast(),
                OUT_CAP as c_int,
            )
        };
        if rc < 0 {
            return Err(SttError::Engine(format!(
                "stream process ({rc}): {}",
                last_error()
            )));
        }
        parse_partial(&self.json())
    }

    fn stream_stop(&mut self) -> Result<Transcript, SttError> {
        self.out[0] = 0;
        // SAFETY: as above.
        let rc = unsafe {
            needle_sys::needle_stream_transcribe_stop(
                self.out.as_mut_ptr().cast(),
                OUT_CAP as c_int,
            )
        };
        if rc < 0 {
            return Err(SttError::Engine(format!(
                "stream stop ({rc}): {}",
                last_error()
            )));
        }
        parse_transcript(&self.json())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_transcribe_json() {
        let j = r#"{"text":" Hola Pando. ","language":"es","ttft_ms":12.5,"decode_tps":90.0,
            "words":[{"word":"Hola","start":0.0,"end":0.4,"probability":0.9}]}"#;
        let t = parse_transcript(j).expect("parse");
        assert_eq!(t.text, "Hola Pando.");
        assert_eq!(t.lang, "es");
        assert_eq!(t.ttft_ms, Some(12.5));
        assert_eq!(t.words.len(), 1);
    }

    #[test]
    fn silence_gives_empty_text() {
        let t = parse_transcript(r#"{"text":"","language":"es","ttft_ms":0.0,"decode_tps":0.0}"#)
            .expect("parse");
        assert!(t.text.is_empty());
        assert!(parse_transcript("").expect("empty").text.is_empty());
    }

    #[test]
    fn parses_stream_json() {
        let j = r#"{"text":"abre","words":[],"pending":"el pro","language":"es","received":1.0,"pass_ms":20.0}"#;
        let p = parse_partial(j).expect("parse");
        assert_eq!((p.text.as_str(), p.pending.as_str()), ("abre", "el pro"));
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(matches!(parse_transcript("{nope"), Err(SttError::Json(_))));
    }
}
