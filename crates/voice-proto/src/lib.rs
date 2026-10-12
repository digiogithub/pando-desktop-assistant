//! App to voice-worker protocol (`pando-stt`, `pando-tts`; ADR 0007).
//!
//! Transport: a byte stream (child-process stdio). Each message is one **frame**:
//! `u32` little-endian payload length, then the [postcard](https://docs.rs/postcard)
//! encoding of [`ToWorker`] (app to worker) or [`FromWorker`] (worker to app).
//!
//! The protocol is deliberately simple (text, PCM, small control structs): never share
//! memory or internal data structures with `pando-tts` (GPL-3, ADR 0007).
//! The first message in each direction is a `Hello` carrying [`PROTOCOL_VERSION`]; peers
//! with a different version must refuse to continue.
//!
//! Audio is **16 kHz mono f32** towards STT; TTS answers with PCM chunks that carry their
//! own sample rate.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// Wire version; bump on any incompatible change to the message enums.
pub const PROTOCOL_VERSION: u16 = 1;

/// Largest accepted frame payload (a 30 s 16 kHz f32 clip is 1.9 MB).
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Sample rate of PCM sent to STT.
pub const STT_SAMPLE_RATE: u32 = 16_000;

/// Request identifier chosen by the app, echoed in every answer.
pub type RequestId = u64;

/// Which worker answered `Hello`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Stt,
    Tts,
}

/// Messages sent by the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToWorker {
    Hello {
        version: u16,
    },
    /// STT: load the model file (`.cact` for Whistle) from `model_path`.
    LoadStt {
        model_path: String,
    },
    /// TTS: load a voice (Piper/Kokoro model directory) from `model_dir`.
    LoadTts {
        model_dir: String,
        voice: String,
    },
    /// STT: one-shot transcription of `pcm` (16 kHz mono f32).
    Transcribe {
        id: RequestId,
        pcm: Vec<f32>,
        /// Forced language code (`es`, `en`, ...); `None` auto-detects.
        lang: Option<String>,
        /// Biasing phrases (project names, agent ids, "Pando").
        keywords: Vec<String>,
        word_ts: bool,
    },
    /// STT: feed a chunk of a live stream (about 1 s); partials come back as `Partial`.
    StreamChunk {
        id: RequestId,
        pcm: Vec<f32>,
        lang: Option<String>,
        keywords: Vec<String>,
    },
    /// STT: end the stream; answered with `Transcript`.
    StreamStop {
        id: RequestId,
    },
    /// TTS: synthesise `text`; answered with `Pcm` chunks and `SynthDone`.
    Synth {
        id: RequestId,
        text: String,
        voice: String,
        lang: Option<String>,
        speed: f32,
    },
    /// Abort the request with this id (synthesis or transcription).
    Cancel {
        id: RequestId,
    },
    Health,
    /// Ask the worker to exit cleanly.
    Shutdown,
}

/// A recognised word with timing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub word: String,
    pub start: f32,
    pub end: f32,
    pub probability: f32,
}

/// Machine-readable error class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    VersionMismatch,
    NotLoaded,
    ModelLoad,
    BadRequest,
    Engine,
    Cancelled,
    Internal,
}

/// Messages sent by a worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FromWorker {
    Hello {
        version: u16,
        role: Role,
    },
    /// Model loaded and ready for requests.
    Ready,
    Transcript {
        id: RequestId,
        text: String,
        lang: String,
        words: Vec<Word>,
        ttft_ms: Option<f32>,
    },
    /// Streaming partial transcript (committed words so far).
    Partial {
        id: RequestId,
        text: String,
    },
    /// VAD / wake-word events from the hands-free path of `pando-stt`.
    SpeechStart,
    SpeechEnd,
    WakeWord {
        score: f32,
    },
    /// One chunk of synthesised mono f32 PCM.
    Pcm {
        id: RequestId,
        sample_rate: u32,
        samples: Vec<f32>,
    },
    SynthDone {
        id: RequestId,
    },
    Error {
        id: Option<RequestId>,
        code: ErrorCode,
        message: String,
    },
    Health {
        role: Role,
        loaded: bool,
        rss_bytes: Option<u64>,
    },
}

/// Protocol failure.
#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("frame of {0} bytes exceeds the {MAX_FRAME_BYTES} byte limit")]
    FrameTooLarge(usize),
    #[error("malformed message: {0}")]
    Decode(postcard::Error),
    #[error("could not encode message: {0}")]
    Encode(postcard::Error),
    #[error("stream ended in the middle of a frame")]
    Truncated,
}

/// Encode one message into a complete frame (length prefix + payload).
pub fn encode_frame<T: Serialize>(msg: &T) -> Result<Vec<u8>, ProtoError> {
    let payload = postcard::to_allocvec(msg).map_err(ProtoError::Encode)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProtoError::FrameTooLarge(payload.len()));
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Decode a frame payload (without the length prefix).
pub fn decode_payload<T: for<'de> Deserialize<'de>>(payload: &[u8]) -> Result<T, ProtoError> {
    postcard::from_bytes(payload).map_err(ProtoError::Decode)
}

/// Write one message and flush.
pub fn write_msg<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), ProtoError> {
    w.write_all(&encode_frame(msg)?)?;
    w.flush()?;
    Ok(())
}

/// Read one message; `Ok(None)` on a clean end of stream before a frame starts.
pub fn read_msg<R: Read, T: for<'de> Deserialize<'de>>(r: &mut R) -> Result<Option<T>, ProtoError> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(ProtoError::Truncated),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let n = u32::from_le_bytes(len) as usize;
    if n > MAX_FRAME_BYTES {
        return Err(ProtoError::FrameTooLarge(n));
    }
    let mut payload = vec![0u8; n];
    r.read_exact(&mut payload).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            ProtoError::Truncated
        } else {
            e.into()
        }
    })?;
    decode_payload(&payload).map(Some)
}

/// Incremental decoder for non-blocking transports: push bytes, pop messages.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Next complete message, `Ok(None)` when more bytes are needed.
    pub fn next_msg<T: for<'de> Deserialize<'de>>(&mut self) -> Result<Option<T>, ProtoError> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let n = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if n > MAX_FRAME_BYTES {
            self.buf.clear();
            return Err(ProtoError::FrameTooLarge(n));
        }
        if self.buf.len() < 4 + n {
            return Ok(None);
        }
        let res = decode_payload(&self.buf[4..4 + n]);
        self.buf.drain(..4 + n);
        res.map(Some)
    }
}

/// Check the peer's `Hello` version against ours.
pub fn check_version(peer: u16) -> Result<(), (ErrorCode, String)> {
    if peer == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err((
            ErrorCode::VersionMismatch,
            format!("peer speaks protocol v{peer}, this build speaks v{PROTOCOL_VERSION}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<ToWorker> {
        vec![
            ToWorker::Hello {
                version: PROTOCOL_VERSION,
            },
            ToWorker::LoadStt {
                model_path: "/m/whistle.cact".into(),
            },
            ToWorker::LoadTts {
                model_dir: "/m/piper".into(),
                voice: "es_ES-davefx-medium".into(),
            },
            ToWorker::Transcribe {
                id: 7,
                pcm: vec![0.0, 0.5, -0.5],
                lang: Some("es".into()),
                keywords: vec!["Pando".into(), "migracion-auth".into()],
                word_ts: true,
            },
            ToWorker::StreamChunk {
                id: 8,
                pcm: vec![0.1; 16],
                lang: None,
                keywords: vec![],
            },
            ToWorker::StreamStop { id: 8 },
            ToWorker::Synth {
                id: 9,
                text: "hola".into(),
                voice: "v".into(),
                lang: None,
                speed: 1.0,
            },
            ToWorker::Cancel { id: 9 },
            ToWorker::Health,
            ToWorker::Shutdown,
        ]
    }

    fn replies() -> Vec<FromWorker> {
        vec![
            FromWorker::Hello {
                version: PROTOCOL_VERSION,
                role: Role::Stt,
            },
            FromWorker::Ready,
            FromWorker::Transcript {
                id: 7,
                text: "hola".into(),
                lang: "es".into(),
                words: vec![Word {
                    word: "hola".into(),
                    start: 0.0,
                    end: 0.4,
                    probability: 0.9,
                }],
                ttft_ms: Some(80.0),
            },
            FromWorker::Partial {
                id: 8,
                text: "ho".into(),
            },
            FromWorker::SpeechStart,
            FromWorker::SpeechEnd,
            FromWorker::WakeWord { score: 0.8 },
            FromWorker::Pcm {
                id: 9,
                sample_rate: 22_050,
                samples: vec![0.0; 100],
            },
            FromWorker::SynthDone { id: 9 },
            FromWorker::Error {
                id: Some(1),
                code: ErrorCode::Engine,
                message: "x".into(),
            },
            FromWorker::Health {
                role: Role::Tts,
                loaded: true,
                rss_bytes: Some(1 << 20),
            },
        ]
    }

    #[test]
    fn round_trip_all_messages() {
        for m in samples() {
            let f = encode_frame(&m).unwrap_or_default();
            let back: ToWorker = decode_payload(&f[4..]).expect("decode");
            assert_eq!(back, m);
        }
        for m in replies() {
            let f = encode_frame(&m).unwrap_or_default();
            let back: FromWorker = decode_payload(&f[4..]).expect("decode");
            assert_eq!(back, m);
        }
    }

    #[test]
    fn stream_read_write() {
        let mut buf = Vec::new();
        for m in samples() {
            write_msg(&mut buf, &m).expect("write");
        }
        let mut cur = io::Cursor::new(buf);
        for m in samples() {
            assert_eq!(read_msg::<_, ToWorker>(&mut cur).expect("read"), Some(m));
        }
        assert_eq!(read_msg::<_, ToWorker>(&mut cur).expect("eof"), None);
    }

    #[test]
    fn truncated_frame_is_an_error() {
        let f = encode_frame(&ToWorker::Health).unwrap_or_default();
        let mut cur = io::Cursor::new(&f[..f.len() - 1]);
        assert!(matches!(
            read_msg::<_, ToWorker>(&mut cur),
            Err(ProtoError::Truncated)
        ));
        let mut cur = io::Cursor::new(&f[..2]);
        assert!(matches!(
            read_msg::<_, ToWorker>(&mut cur),
            Err(ProtoError::Truncated)
        ));
    }

    #[test]
    fn oversized_frame_rejected() {
        let mut bytes = ((MAX_FRAME_BYTES + 1) as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0; 8]);
        let mut cur = io::Cursor::new(bytes.clone());
        assert!(matches!(
            read_msg::<_, ToWorker>(&mut cur),
            Err(ProtoError::FrameTooLarge(_))
        ));
        let mut d = FrameDecoder::new();
        d.push(&bytes);
        assert!(matches!(
            d.next_msg::<ToWorker>(),
            Err(ProtoError::FrameTooLarge(_))
        ));
    }

    #[test]
    fn incremental_decoder_handles_byte_by_byte() {
        let mut d = FrameDecoder::new();
        let mut all = Vec::new();
        for m in replies() {
            all.extend(encode_frame(&m).unwrap_or_default());
        }
        let mut out = Vec::new();
        for b in all {
            d.push(&[b]);
            while let Some(m) = d.next_msg::<FromWorker>().expect("decode") {
                out.push(m);
            }
        }
        assert_eq!(out, replies());
    }

    #[test]
    fn version_check() {
        assert!(check_version(PROTOCOL_VERSION).is_ok());
        assert_eq!(
            check_version(99).err().map(|e| e.0),
            Some(ErrorCode::VersionMismatch)
        );
    }

    /// Cheap deterministic fuzz: random bytes and mutated valid frames never panic.
    #[test]
    fn decoder_fuzz_does_not_panic() {
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let valid: Vec<Vec<u8>> = samples()
            .iter()
            .map(|m| encode_frame(m).unwrap_or_default())
            .collect();
        for i in 0..20_000 {
            let bytes: Vec<u8> = if i % 2 == 0 {
                let n = (next() % 64) as usize;
                (0..n).map(|_| next() as u8).collect()
            } else {
                let mut v = valid[(next() as usize) % valid.len()].clone();
                for _ in 0..(1 + next() % 4) {
                    let k = (next() as usize) % v.len();
                    v[k] = next() as u8;
                }
                v
            };
            let _ = decode_payload::<ToWorker>(&bytes);
            let _ = decode_payload::<FromWorker>(&bytes);
            let mut cur = io::Cursor::new(bytes.clone());
            let _ = read_msg::<_, ToWorker>(&mut cur);
            let mut d = FrameDecoder::new();
            d.push(&bytes);
            let _ = d.next_msg::<FromWorker>();
        }
    }
}
