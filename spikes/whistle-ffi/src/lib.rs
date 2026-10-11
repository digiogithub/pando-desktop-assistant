//! Spike: safe wrapper over the Whistle speech model in `libneedle.a`.
//!
//! The engine is process-global and not thread-safe, so `Whistle` is `!Send + !Sync` and only one
//! can exist per process (guarded by an atomic flag). Production code owns it on one thread.
pub mod sys;

use std::ffi::{CStr, CString, c_char, c_int};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};

static LOADED: AtomicBool = AtomicBool::new(false);
const OUT_CAP: usize = 1 << 18; // same 256 KiB buffer as the Python wrapper

#[derive(Debug)]
pub struct Whistle {
    out: Vec<u8>,
    _not_send: PhantomData<*const ()>,
}

fn last_error() -> String {
    // SAFETY: the runtime returns a NUL-terminated string (or NULL) valid until the next call.
    unsafe {
        let p = sys::needle_last_error();
        if p.is_null() {
            "unknown".into()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

fn opt_cstring(s: Option<&str>) -> Result<Option<CString>, String> {
    s.map(|v| CString::new(v).map_err(|e| e.to_string()))
        .transpose()
}

fn ptr(s: &Option<CString>) -> *const c_char {
    s.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
}

impl Whistle {
    /// Loads a `.cact` speech model from memory.
    pub fn load(cact: &[u8]) -> Result<Self, String> {
        if LOADED.swap(true, Ordering::SeqCst) {
            return Err("Whistle already loaded in this process".into());
        }
        // SAFETY: pointer/len come from a live slice; the engine copies what it needs.
        let rc = unsafe { sys::needle_load(cact.as_ptr(), cact.len() as u64) };
        if rc < 0 {
            LOADED.store(false, Ordering::SeqCst);
            return Err(format!("needle_load failed ({rc}): {}", last_error()));
        }
        // SAFETY: plain query.
        if unsafe { sys::needle_models() } & sys::NEEDLE_SPEECH == 0 {
            return Err("the .cact holds no speech model".into());
        }
        Ok(Self {
            out: vec![0; OUT_CAP],
            _not_send: PhantomData,
        })
    }

    fn take_json(&self) -> String {
        let end = self
            .out
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.out.len());
        String::from_utf8_lossy(&self.out[..end]).into_owned()
    }

    /// Transcribes 16 kHz mono f32 PCM. Returns the engine JSON verbatim.
    pub fn transcribe(
        &mut self,
        pcm: &[f32],
        language: Option<&str>,
        keywords: Option<&str>,
        words: bool,
    ) -> Result<String, String> {
        let lang = opt_cstring(language)?;
        let kw = opt_cstring(keywords)?;
        self.out[0] = 0;
        // SAFETY: buffers are live for the call; out capacity is the buffer length.
        let rc = unsafe {
            sys::needle_transcribe(
                pcm.as_ptr(),
                pcm.len() as c_int,
                ptr(&lang),
                ptr(&kw),
                words as c_int,
                self.out.as_mut_ptr().cast(),
                OUT_CAP as c_int,
            )
        };
        if rc < 0 {
            return Err(format!("needle_transcribe failed ({rc}): {}", last_error()));
        }
        Ok(self.take_json())
    }

    /// Feeds ~1 s of PCM to the live transcriber.
    pub fn stream_process(
        &mut self,
        pcm: &[f32],
        language: Option<&str>,
        keywords: Option<&str>,
    ) -> Result<String, String> {
        let lang = opt_cstring(language)?;
        let kw = opt_cstring(keywords)?;
        self.out[0] = 0;
        // SAFETY: as above.
        let rc = unsafe {
            sys::needle_stream_transcribe_process(
                pcm.as_ptr(),
                pcm.len() as c_int,
                ptr(&lang),
                ptr(&kw),
                self.out.as_mut_ptr().cast(),
                OUT_CAP as c_int,
            )
        };
        if rc < 0 {
            return Err(format!("stream failed ({rc}): {}", last_error()));
        }
        Ok(self.take_json())
    }

    pub fn stream_stop(&mut self) -> Result<String, String> {
        self.out[0] = 0;
        // SAFETY: as above.
        let rc = unsafe {
            sys::needle_stream_transcribe_stop(self.out.as_mut_ptr().cast(), OUT_CAP as c_int)
        };
        if rc < 0 {
            return Err(format!("stream stop failed ({rc}): {}", last_error()));
        }
        Ok(self.take_json())
    }

    /// Number of floats `needle_embed` would return for this clip (one row per 80 ms).
    pub fn embed_len(&mut self, pcm: &[f32]) -> i32 {
        // SAFETY: null output asks only for the size.
        unsafe {
            sys::needle_embed(
                std::ptr::null(),
                pcm.as_ptr(),
                pcm.len() as c_int,
                std::ptr::null_mut(),
                0,
            )
        }
    }
}

/// Reads a 16 kHz mono WAV (int16 or f32) into f32 samples.
pub fn read_wav(path: &str) -> Result<Vec<f32>, String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("{path}: {e}"))?;
    let spec = r.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 {
        return Err(format!(
            "{path}: need 16 kHz mono, got {} Hz x{}",
            spec.sample_rate, spec.channels
        ));
    }
    match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string()),
        hound::SampleFormat::Int => {
            let scale = (1u32 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())
        }
    }
}
