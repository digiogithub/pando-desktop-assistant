//! Microphone capture: cpal input at the device's native rate -> lock-free ring -> supervisor
//! thread (downmix already done in the callback) -> rubato -> 16 kHz mono frames of 20 ms.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat};

use crate::device::{find_device, open_host};
use crate::level::{LevelMeter, rms};
use crate::preroll::PreRoll;
use crate::resample::StreamResampler;
use crate::{AudioError, CAPTURE_RATE};

/// Frame size handed to the consumer: 20 ms at 16 kHz.
pub const FRAME_SAMPLES: usize = (CAPTURE_RATE as usize) / 50;

/// Capture options.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Device id or name; `None` = host default (follows the system default).
    pub device: Option<String>,
    /// Host name (`PipeWire`, `PulseAudio`, `Alsa`); `None` = platform default.
    pub host: Option<String>,
    /// Pre-roll kept for the consumer, seconds.
    pub preroll_secs: f32,
    /// Rebuild the stream when no callback arrived for this long.
    pub watchdog: Duration,
    /// Echo canceller applied to the 16 kHz stream before level, pre-roll and `on_frame`.
    #[cfg(feature = "echo-cancel")]
    pub echo: Option<crate::aec::EchoHandle>,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            device: None,
            host: None,
            preroll_secs: 2.0,
            watchdog: Duration::from_millis(500),
            #[cfg(feature = "echo-cancel")]
            echo: None,
        }
    }
}

/// Lifecycle notifications.
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureEvent {
    /// A stream is running on `device` at `native_rate`.
    Started { device: String, native_rate: u32 },
    /// The stream was rebuilt (`reason`: `error`, `stalled`).
    Restarted { reason: String },
    /// Opening failed; retrying every second.
    Error(String),
}

/// Real-time state shared with the cpal callback. No allocation, no locks.
pub(crate) struct InputCore {
    pub producer: rtrb::Producer<f32>,
    pub channels: usize,
    pub callbacks: Arc<AtomicU64>,
    pub dropped: Arc<AtomicU64>,
}

impl InputCore {
    /// Called from the audio callback: downmix to mono and push into the ring.
    pub fn process<T: Copy>(&mut self, data: &[T])
    where
        f32: FromSample<T>,
    {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        let inv = 1.0 / self.channels as f32;
        for frame in data.chunks_exact(self.channels) {
            let mut acc = 0.0f32;
            for &s in frame {
                acc += f32::from_sample(s);
            }
            if self.producer.push(acc * inv).is_err() {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

struct Session {
    _stream: cpal::Stream,
    consumer: rtrb::Consumer<f32>,
    native_rate: u32,
    device_name: String,
    callbacks: Arc<AtomicU64>,
    invalid: Arc<AtomicBool>,
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut core: InputCore,
    invalid: Arc<AtomicBool>,
) -> Result<cpal::Stream, AudioError>
where
    T: cpal::SizedSample + Copy + Send + 'static,
    f32: FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| core.process(data),
            move |e| {
                tracing::warn!("input stream error: {e}");
                invalid.store(true, Ordering::SeqCst);
            },
            None,
        )
        .map_err(|e| AudioError::Backend(e.to_string()))
}

fn open_session(cfg: &CaptureConfig) -> Result<Session, AudioError> {
    // A fresh host each time: some backends snapshot the device list at creation (S4).
    let host = open_host(cfg.host.as_deref())?;
    let device = find_device(&host, cfg.device.as_deref(), true)?;
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::Backend(e.to_string()))?;
    let config = supported.config();
    let native_rate = config.sample_rate;
    let channels = config.channels as usize;
    let (producer, consumer) = rtrb::RingBuffer::<f32>::new(native_rate as usize * 2);
    let callbacks = Arc::new(AtomicU64::new(0));
    let invalid = Arc::new(AtomicBool::new(false));
    let core = InputCore {
        producer,
        channels,
        callbacks: callbacks.clone(),
        dropped: Arc::new(AtomicU64::new(0)),
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_stream::<f32>(&device, &config, core, invalid.clone())?,
        SampleFormat::I16 => build_stream::<i16>(&device, &config, core, invalid.clone())?,
        SampleFormat::I32 => build_stream::<i32>(&device, &config, core, invalid.clone())?,
        SampleFormat::U16 => build_stream::<u16>(&device, &config, core, invalid.clone())?,
        SampleFormat::I24 => build_stream::<cpal::I24>(&device, &config, core, invalid.clone())?,
        SampleFormat::F64 => build_stream::<f64>(&device, &config, core, invalid.clone())?,
        other => return Err(AudioError::UnsupportedFormat(format!("{other:?}"))),
    };
    stream
        .play()
        .map_err(|e| AudioError::Backend(e.to_string()))?;
    Ok(Session {
        _stream: stream,
        consumer,
        native_rate,
        device_name: device
            .description()
            .map(|d| d.to_string())
            .unwrap_or_default(),
        callbacks,
        invalid,
    })
}

/// A running capture. Dropping it stops the stream and joins the supervisor thread.
pub struct Capture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    preroll: Arc<Mutex<PreRoll>>,
    level: Arc<AtomicU32>,
    restarts: Arc<AtomicU64>,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Capture")
    }
}

impl Capture {
    /// Start capturing. `on_frame` receives 20 ms frames (320 samples) of 16 kHz mono f32 on the
    /// supervisor thread; `on_event` receives lifecycle events. Both must be quick.
    pub fn start(
        cfg: CaptureConfig,
        mut on_frame: impl FnMut(&[f32]) + Send + 'static,
        mut on_event: impl FnMut(CaptureEvent) + Send + 'static,
    ) -> Capture {
        let stop = Arc::new(AtomicBool::new(false));
        let preroll = Arc::new(Mutex::new(PreRoll::new(cfg.preroll_secs, CAPTURE_RATE)));
        let level = Arc::new(AtomicU32::new(0));
        let restarts = Arc::new(AtomicU64::new(0));
        let (s, p, l, r) = (
            stop.clone(),
            preroll.clone(),
            level.clone(),
            restarts.clone(),
        );
        let thread = std::thread::Builder::new()
            .name("audio-capture".into())
            .spawn(move || supervise(cfg, &s, &p, &l, &r, &mut on_frame, &mut on_event))
            .ok();
        Capture {
            stop,
            thread,
            preroll,
            level,
            restarts,
        }
    }

    /// Last normalised microphone level (0..=1), updated every 20 ms.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// The last `preroll_secs` of 16 kHz audio, oldest first.
    pub fn preroll(&self) -> Vec<f32> {
        self.preroll
            .lock()
            .map(|p| p.snapshot())
            .unwrap_or_default()
    }

    /// How many times the stream was rebuilt after the first start.
    pub fn restarts(&self) -> u64 {
        self.restarts.load(Ordering::Relaxed)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn supervise(
    cfg: CaptureConfig,
    stop: &AtomicBool,
    preroll: &Mutex<PreRoll>,
    level: &AtomicU32,
    restarts: &AtomicU64,
    on_frame: &mut dyn FnMut(&[f32]),
    on_event: &mut dyn FnMut(CaptureEvent),
) {
    let mut meter = LevelMeter::default();
    let mut first = true;
    let mut reason = String::new();
    while !stop.load(Ordering::SeqCst) {
        let mut session = match open_session(&cfg) {
            Ok(s) => s,
            Err(e) => {
                on_event(CaptureEvent::Error(e.to_string()));
                sleep_unless_stopped(stop, Duration::from_secs(1));
                continue;
            }
        };
        if first {
            first = false;
        } else {
            restarts.fetch_add(1, Ordering::Relaxed);
            on_event(CaptureEvent::Restarted {
                reason: std::mem::take(&mut reason),
            });
        }
        on_event(CaptureEvent::Started {
            device: session.device_name.clone(),
            native_rate: session.native_rate,
        });
        let mut resampler = match StreamResampler::new(session.native_rate, CAPTURE_RATE) {
            Ok(r) => r,
            Err(e) => {
                on_event(CaptureEvent::Error(e.to_string()));
                sleep_unless_stopped(stop, Duration::from_secs(1));
                continue;
            }
        };
        let mut native = Vec::<f32>::with_capacity(4096);
        let mut mono16 = Vec::<f32>::with_capacity(4096);
        let mut pending = Vec::<f32>::with_capacity(2 * FRAME_SAMPLES);
        let mut last_cb = session.callbacks.load(Ordering::Relaxed);
        let mut last_change = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
            native.clear();
            let n = session.consumer.slots();
            if let Ok(chunk) = session.consumer.read_chunk(n) {
                let (a, b) = chunk.as_slices();
                native.extend_from_slice(a);
                native.extend_from_slice(b);
                chunk.commit_all();
            }
            if !native.is_empty() {
                mono16.clear();
                if resampler.process(&native, &mut mono16).is_ok() {
                    pending.extend_from_slice(&mono16);
                }
                while pending.len() >= FRAME_SAMPLES {
                    #[allow(unused_mut)]
                    let mut frame: Vec<f32> = pending.drain(..FRAME_SAMPLES).collect();
                    #[cfg(feature = "echo-cancel")]
                    if let Some(echo) = &cfg.echo
                        && let Ok(mut e) = echo.lock()
                    {
                        e.set_active(!crate::aec::looks_like_headset(&session.device_name));
                        e.process(&mut frame);
                    }
                    let lvl = meter.update(rms(&frame));
                    level.store(lvl.to_bits(), Ordering::Relaxed);
                    if let Ok(mut p) = preroll.lock() {
                        p.push(&frame);
                    }
                    on_frame(&frame);
                }
            }
            // Watchdog: explicit error, or no callback for `cfg.watchdog` (PipeWire stalls
            // silently when an explicit device disappears, S4).
            let cb = session.callbacks.load(Ordering::Relaxed);
            if cb != last_cb {
                last_cb = cb;
                last_change = Instant::now();
            }
            if session.invalid.load(Ordering::SeqCst) {
                reason = "error".into();
                break;
            }
            if last_change.elapsed() > cfg.watchdog {
                reason = "stalled".into();
                break;
            }
        }
        // `session` (and its stream) drops here, before the next open.
    }
}

pub(crate) fn sleep_unless_stopped(stop: &AtomicBool, total: Duration) {
    let end = Instant::now() + total;
    while Instant::now() < end && !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(channels: usize, cap: usize) -> (InputCore, rtrb::Consumer<f32>) {
        let (p, c) = rtrb::RingBuffer::new(cap);
        (
            InputCore {
                producer: p,
                channels,
                callbacks: Arc::default(),
                dropped: Arc::default(),
            },
            c,
        )
    }

    #[test]
    fn stereo_is_downmixed_to_mono() {
        let (mut c, mut rx) = core(2, 16);
        c.process::<f32>(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0]);
        let got: Vec<f32> = std::iter::from_fn(|| rx.pop().ok()).collect();
        assert_eq!(got, vec![0.5, 0.5, 0.0]);
        assert_eq!(c.callbacks.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn integer_formats_are_converted() {
        let (mut c, mut rx) = core(1, 8);
        c.process::<i16>(&[i16::MAX, i16::MIN, 0]);
        let got: Vec<f32> = std::iter::from_fn(|| rx.pop().ok()).collect();
        assert!((got[0] - 1.0).abs() < 1e-3 && (got[1] + 1.0).abs() < 1e-3 && got[2] == 0.0);
    }

    #[test]
    fn full_ring_drops_instead_of_blocking() {
        let (mut c, _rx) = core(1, 4);
        c.process::<f32>(&[0.1; 10]);
        assert_eq!(c.dropped.load(Ordering::Relaxed), 6);
    }
}
