//! Speaker playback: PCM at the voice rate -> rubato -> lock-free ring -> cpal output callback.
//!
//! `stop()` flips atomics; the very next callback drops everything queued in the ring and
//! writes silence (one buffer, 10-25 ms, plus the device's own latency). The output RMS is
//! computed in the callback on the samples it writes, so it matches what is audible up to
//! [`Playback::output_latency`].

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat};

use crate::AudioError;
use crate::capture::sleep_unless_stopped;
use crate::device::{find_device, open_host};

/// Shared handle to the exact PCM that leaves the output callback (mono, device rate, silence
/// included so the timeline is continuous). The echo canceller of [`crate::Capture`] reads it.
type RefSlot = Arc<Mutex<Option<(rtrb::Consumer<f32>, u32)>>>;

#[derive(Clone, Default)]
pub struct ReferenceTap(RefSlot);

impl std::fmt::Debug for ReferenceTap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReferenceTap")
    }
}

impl ReferenceTap {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn install(&self, consumer: rtrb::Consumer<f32>, rate: u32) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some((consumer, rate));
        }
    }

    /// Move everything available into `out`; returns the device rate of those samples
    /// (`None` while no playback stream is open).
    pub fn drain_into(&self, out: &mut Vec<f32>) -> Option<u32> {
        let mut g = self.0.lock().ok()?;
        let (c, rate) = g.as_mut()?;
        let n = c.slots();
        if n > 0
            && let Ok(chunk) = c.read_chunk(n)
        {
            let (a, b) = chunk.as_slices();
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            chunk.commit_all();
        }
        Some(*rate)
    }
}

/// Playback options.
#[derive(Debug, Clone)]
pub struct PlaybackConfig {
    pub device: Option<String>,
    pub host: Option<String>,
    /// Ring size in seconds of device-rate audio.
    pub ring_secs: f32,
    pub watchdog: Duration,
    /// Publish the played PCM for echo cancellation.
    pub reference: Option<ReferenceTap>,
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            device: None,
            host: None,
            ring_secs: 3.0,
            watchdog: Duration::from_millis(500),
            reference: None,
        }
    }
}

/// Playback notifications.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackEvent {
    /// The first queued audio reached the output callback.
    Started,
    /// Everything queued has been played (or dropped by `stop()`).
    Drained,
    /// The output stream was (re)built on `device`.
    DeviceReady {
        device: String,
        native_rate: u32,
    },
    Error(String),
}

/// Atomics shared between the callback, the feeder thread and the handle.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    /// Bumped by `stop()`: chunks stamped with an older epoch are discarded.
    pub epoch: AtomicU64,
    /// Bumped to ask the callback to empty the ring.
    pub drain_gen: AtomicU64,
    /// Device-rate frames pushed into the ring.
    pub submitted: AtomicU64,
    /// Frames the callback took from the ring and played.
    pub played: AtomicU64,
    /// Frames played plus frames dropped by a drain.
    pub consumed: AtomicU64,
    /// f32 bits of the RMS of the last callback buffer.
    pub rms_bits: AtomicU32,
    /// Output latency reported by the backend, microseconds.
    pub latency_us: AtomicU32,
    pub callbacks: AtomicU64,
    pub invalid: AtomicBool,
    /// Output is ducked by 12 dB (hands-free barge-in).
    pub duck: AtomicBool,
}

/// Real-time output state. No allocation, no locks.
pub(crate) struct OutputCore {
    pub consumer: rtrb::Consumer<f32>,
    pub channels: usize,
    pub shared: Arc<Shared>,
    pub seen_drain: u64,
    /// Mirror of every mono frame written (echo reference); lock- and allocation-free.
    pub reference: Option<rtrb::Producer<f32>>,
}

impl OutputCore {
    /// Fill one output buffer.
    pub fn fill<T: Copy + Sample + FromSample<f32>>(&mut self, out: &mut [T]) {
        let sh = &*self.shared;
        sh.callbacks.fetch_add(1, Ordering::Relaxed);
        let gen_now = sh.drain_gen.load(Ordering::Acquire);
        if gen_now != self.seen_drain {
            self.seen_drain = gen_now;
            let n = self.consumer.slots();
            if n > 0
                && let Ok(chunk) = self.consumer.read_chunk(n)
            {
                chunk.commit_all();
                sh.consumed.fetch_add(n as u64, Ordering::Relaxed);
            }
        }
        let gain = if sh.duck.load(Ordering::Relaxed) {
            crate::bargein::BargeIn::DUCK_GAIN
        } else {
            1.0
        };
        let mut sumsq = 0.0f32;
        let mut played = 0u64;
        let frames = out.len() / self.channels;
        for frame in out.chunks_exact_mut(self.channels) {
            let v = match self.consumer.pop() {
                Ok(v) => {
                    played += 1;
                    v * gain
                }
                Err(_) => 0.0,
            };
            sumsq += v * v;
            if let Some(r) = self.reference.as_mut() {
                let _ = r.push(v);
            }
            let s = T::from_sample(v);
            for o in frame {
                *o = s;
            }
        }
        if played > 0 {
            sh.played.fetch_add(played, Ordering::Relaxed);
            sh.consumed.fetch_add(played, Ordering::Relaxed);
        }
        let rms = if frames > 0 {
            (sumsq / frames as f32).sqrt()
        } else {
            0.0
        };
        sh.rms_bits.store(rms.to_bits(), Ordering::Relaxed);
    }
}

enum Cmd {
    Play {
        epoch: u64,
        samples: Vec<f32>,
        rate: u32,
    },
}

struct Session {
    _stream: cpal::Stream,
    producer: rtrb::Producer<f32>,
    native_rate: u32,
    device_name: String,
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut core: OutputCore,
) -> Result<cpal::Stream, AudioError>
where
    T: cpal::SizedSample + Copy + Send + FromSample<f32> + 'static,
{
    let sh = core.shared.clone();
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let ts = info.timestamp();
                let d = ts.playback.duration_since(ts.callback);
                core.shared.latency_us.store(
                    d.as_micros().min(u32::MAX as u128) as u32,
                    Ordering::Relaxed,
                );
                core.fill(data);
            },
            move |e| {
                tracing::warn!("output stream error: {e}");
                sh.invalid.store(true, Ordering::SeqCst);
            },
            None,
        )
        .map_err(|e| AudioError::Backend(e.to_string()))
}

fn open_session(cfg: &PlaybackConfig, shared: &Arc<Shared>) -> Result<Session, AudioError> {
    let host = open_host(cfg.host.as_deref())?;
    let device = find_device(&host, cfg.device.as_deref(), false)?;
    let supported = device
        .default_output_config()
        .map_err(|e| AudioError::Backend(e.to_string()))?;
    let config = supported.config();
    let native_rate = config.sample_rate;
    let (producer, consumer) =
        rtrb::RingBuffer::<f32>::new((native_rate as f32 * cfg.ring_secs) as usize);
    shared.invalid.store(false, Ordering::SeqCst);
    let reference = cfg.reference.as_ref().map(|tap| {
        let (p, c) = rtrb::RingBuffer::<f32>::new(native_rate as usize);
        tap.install(c, native_rate);
        p
    });
    let core = OutputCore {
        consumer,
        channels: config.channels as usize,
        shared: shared.clone(),
        seen_drain: shared.drain_gen.load(Ordering::Acquire),
        reference,
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build_stream::<f32>(&device, &config, core)?,
        SampleFormat::I16 => build_stream::<i16>(&device, &config, core)?,
        SampleFormat::I32 => build_stream::<i32>(&device, &config, core)?,
        SampleFormat::U16 => build_stream::<u16>(&device, &config, core)?,
        SampleFormat::I24 => build_stream::<cpal::I24>(&device, &config, core)?,
        SampleFormat::F64 => build_stream::<f64>(&device, &config, core)?,
        other => return Err(AudioError::UnsupportedFormat(format!("{other:?}"))),
    };
    stream
        .play()
        .map_err(|e| AudioError::Backend(e.to_string()))?;
    Ok(Session {
        _stream: stream,
        producer,
        native_rate,
        device_name: device
            .description()
            .map(|d| d.to_string())
            .unwrap_or_default(),
    })
}

/// Handle to the playback thread.
pub struct Playback {
    shared: Arc<Shared>,
    tx: Sender<Cmd>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Playback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Playback")
    }
}

impl Playback {
    pub fn start(
        cfg: PlaybackConfig,
        mut on_event: impl FnMut(PlaybackEvent) + Send + 'static,
    ) -> Playback {
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let (sh, st) = (shared.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("audio-playback".into())
            .spawn(move || feeder(cfg, &sh, &st, &rx, &mut on_event))
            .ok();
        Playback {
            shared,
            tx,
            stop,
            thread,
        }
    }

    /// Queue mono PCM recorded at `rate` Hz (the voice rate). Returns immediately.
    pub fn play(&self, samples: Vec<f32>, rate: u32) {
        let epoch = self.shared.epoch.load(Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Play {
            epoch,
            samples,
            rate,
        });
    }

    /// Stop now: queued audio is discarded and the next output callback writes silence.
    pub fn stop(&self) {
        self.shared.epoch.fetch_add(1, Ordering::SeqCst);
        self.shared.drain_gen.fetch_add(1, Ordering::Release);
    }

    /// Duck the output by 12 dB (`true`) or restore it; takes effect on the next callback.
    pub fn set_duck(&self, on: bool) {
        self.shared.duck.store(on, Ordering::Relaxed);
    }

    /// RMS of the last output buffer (raw, not normalised).
    pub fn output_rms(&self) -> f32 {
        f32::from_bits(self.shared.rms_bits.load(Ordering::Relaxed))
    }

    /// Latency between the callback and the sound leaving the device, as reported by the backend.
    pub fn output_latency(&self) -> Duration {
        Duration::from_micros(u64::from(self.shared.latency_us.load(Ordering::Relaxed)))
    }

    /// True while queued audio has not been consumed yet.
    pub fn is_playing(&self) -> bool {
        self.shared.consumed.load(Ordering::Relaxed) < self.shared.submitted.load(Ordering::Relaxed)
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Chunk {
    samples: Vec<f32>,
    pos: usize,
}

fn feeder(
    cfg: PlaybackConfig,
    sh: &Arc<Shared>,
    stop: &AtomicBool,
    rx: &Receiver<Cmd>,
    on_event: &mut dyn FnMut(PlaybackEvent),
) {
    let mut queue: VecDeque<Chunk> = VecDeque::new();
    let mut current_epoch = sh.epoch.load(Ordering::SeqCst);
    let mut playing = false;
    let mut idle_played = sh.played.load(Ordering::Relaxed);
    'outer: while !stop.load(Ordering::SeqCst) {
        let mut session = match open_session(&cfg, sh) {
            Ok(s) => s,
            Err(e) => {
                on_event(PlaybackEvent::Error(e.to_string()));
                sleep_unless_stopped(stop, Duration::from_secs(1));
                continue;
            }
        };
        on_event(PlaybackEvent::DeviceReady {
            device: session.device_name.clone(),
            native_rate: session.native_rate,
        });
        let mut last_cb = sh.callbacks.load(Ordering::Relaxed);
        let mut last_change = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            // 1. new chunks (resampled here, never in the callback)
            match rx.recv_timeout(Duration::from_millis(2)) {
                Ok(Cmd::Play {
                    epoch,
                    samples,
                    rate,
                }) => {
                    if epoch == sh.epoch.load(Ordering::SeqCst) {
                        enqueue(&mut queue, samples, rate, session.native_rate);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break 'outer,
            }
            // 2. stop() discards what is still queued
            let e = sh.epoch.load(Ordering::SeqCst);
            if e != current_epoch {
                current_epoch = e;
                queue.clear();
            }
            // 3. move audio into the ring
            while let Some(front) = queue.front_mut() {
                let free = session.producer.slots();
                if free == 0 {
                    break;
                }
                let n = free.min(front.samples.len() - front.pos).min(2048);
                let epoch_before = sh.epoch.load(Ordering::SeqCst);
                if let Ok(mut w) = session.producer.write_chunk_uninit(n) {
                    let (a, b) = w.as_mut_slices();
                    let src = &front.samples[front.pos..front.pos + n];
                    let (s1, s2) = src.split_at(a.len());
                    for (d, s) in a.iter_mut().zip(s1) {
                        d.write(*s);
                    }
                    for (d, s) in b.iter_mut().zip(s2) {
                        d.write(*s);
                    }
                    // SAFETY: every slot of both slices was initialised above (a.len()+b.len() == n).
                    unsafe { w.commit_all() };
                    sh.submitted.fetch_add(n as u64, Ordering::Relaxed);
                }
                front.pos += n;
                if front.pos >= front.samples.len() {
                    queue.pop_front();
                }
                // stop() raced with this push: ask the callback to empty the ring again.
                if sh.epoch.load(Ordering::SeqCst) != epoch_before {
                    sh.drain_gen.fetch_add(1, Ordering::Release);
                    break;
                }
            }
            // 4. events
            let played = sh.played.load(Ordering::Relaxed);
            if !playing && played > idle_played {
                playing = true;
                on_event(PlaybackEvent::Started);
            }
            if playing
                && queue.is_empty()
                && sh.consumed.load(Ordering::Relaxed) >= sh.submitted.load(Ordering::Relaxed)
            {
                playing = false;
                idle_played = sh.played.load(Ordering::Relaxed);
                on_event(PlaybackEvent::Drained);
            }
            // 5. watchdog
            let cb = sh.callbacks.load(Ordering::Relaxed);
            if cb != last_cb {
                last_cb = cb;
                last_change = Instant::now();
            }
            if sh.invalid.load(Ordering::SeqCst) || last_change.elapsed() > cfg.watchdog {
                // Audio still in the ring is lost with the stream; account for it.
                let sub = sh.submitted.load(Ordering::Relaxed);
                sh.consumed.store(sub, Ordering::Relaxed);
                break;
            }
        }
    }
}

/// Resample a chunk to the device rate and queue it. Each chunk is resampled as a whole clip
/// (a TTS sentence), which keeps this stateless; the filter edge is a few milliseconds.
fn enqueue(queue: &mut VecDeque<Chunk>, samples: Vec<f32>, rate: u32, native: u32) {
    let samples = if rate == native {
        samples
    } else {
        match crate::resample::resample_all(&samples, rate, native) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("cannot resample {rate} -> {native}: {e}");
                return;
            }
        }
    };
    if !samples.is_empty() {
        queue.push_back(Chunk { samples, pos: 0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(channels: usize, cap: usize) -> (OutputCore, rtrb::Producer<f32>, Arc<Shared>) {
        let (p, c) = rtrb::RingBuffer::new(cap);
        let sh = Arc::new(Shared::default());
        (
            OutputCore {
                consumer: c,
                channels,
                shared: sh.clone(),
                seen_drain: 0,
                reference: None,
            },
            p,
            sh,
        )
    }

    #[test]
    fn mono_is_duplicated_and_silence_fills_underruns() {
        let (mut c, mut tx, sh) = core(2, 16);
        for v in [0.5, -0.5] {
            tx.push(v).expect("room");
        }
        let mut out = [9.0f32; 6];
        c.fill(&mut out);
        assert_eq!(out, [0.5, 0.5, -0.5, -0.5, 0.0, 0.0]);
        assert_eq!(sh.played.load(Ordering::Relaxed), 2);
        assert_eq!(sh.consumed.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn rms_matches_what_was_written() {
        let (mut c, mut tx, sh) = core(1, 64);
        for _ in 0..32 {
            tx.push(0.5).expect("room");
        }
        let mut out = [0.0f32; 32];
        c.fill(&mut out);
        let rms = f32::from_bits(sh.rms_bits.load(Ordering::Relaxed));
        assert!((rms - 0.5).abs() < 1e-6);
        let mut out = [0.0f32; 32];
        c.fill(&mut out); // ring empty: silence
        assert_eq!(f32::from_bits(sh.rms_bits.load(Ordering::Relaxed)), 0.0);
    }

    #[test]
    fn drain_request_empties_the_ring_in_one_callback() {
        let (mut c, mut tx, sh) = core(1, 4096);
        for _ in 0..4000 {
            tx.push(0.3).expect("room");
        }
        sh.drain_gen.fetch_add(1, Ordering::Release); // what stop() does
        let mut out = [1.0f32; 64];
        c.fill(&mut out);
        assert!(out.iter().all(|&s| s == 0.0), "silence right after stop");
        assert_eq!(sh.consumed.load(Ordering::Relaxed), 4000);
        assert_eq!(sh.played.load(Ordering::Relaxed), 0);
        assert_eq!(c.consumer.slots(), 0);
        // and playback works again afterwards
        tx.push(0.7).expect("room");
        c.fill(&mut out);
        assert_eq!(out[0], 0.7);
    }

    #[test]
    fn reference_mirrors_exactly_what_is_written_including_silence() {
        let (mut c, mut tx, _sh) = core(2, 16);
        let (rp, mut rc) = rtrb::RingBuffer::new(16);
        c.reference = Some(rp);
        tx.push(0.5).expect("room");
        let mut out = [0.0f32; 6]; // 3 frames: one sample then two underruns
        c.fill(&mut out);
        let got: Vec<f32> = std::iter::from_fn(|| rc.pop().ok()).collect();
        assert_eq!(got, vec![0.5, 0.0, 0.0]);
    }

    #[test]
    fn integer_output_format() {
        let (mut c, mut tx, _sh) = core(1, 8);
        tx.push(1.0).expect("room");
        let mut out = [0i16; 2];
        c.fill(&mut out);
        assert!(out[0] > 32000 && out[1] == 0);
    }
}
