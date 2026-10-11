//! Spike S4 (PDA-T-0021): cpal 0.18 capture/playback, rubato 5 FFT resampling,
//! sonora AEC3 with the played TTS as render reference, barge-in feasibility.
//!
//! Usage:
//!   s4-audio-aec devices
//!   s4-audio-aec duplex <alsa|pipewire|pulseaudio> <play.wav> <gain> <secs> <out_dir>
//!   s4-audio-aec resample
//!   s4-audio-aec aec <out_dir> [near.wav] [near_at_s] [near_gain]
//!   s4-audio-aec hotplug <pipewire|pulseaudio>
//!
//! Spike code: `unwrap`/`expect` are fine here, this never ships.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use audioadapter_buffers::direct::InterleavedSlice;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{HostId, SampleFormat, StreamInstant};
use rubato::{Fft, FixedSync, Resampler};

fn host(name: &str) -> cpal::Host {
    let id = cpal::available_hosts()
        .into_iter()
        .find(|h| format!("{h:?}").eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("host {name} not available: {:?}", cpal::available_hosts()));
    cpal::host_from_id(id).unwrap()
}

fn devices() {
    println!(
        "available hosts: {:?}; default host: {:?}",
        cpal::available_hosts(),
        cpal::default_host().id()
    );
    for id in cpal::available_hosts() {
        let t = Instant::now();
        let h = match cpal::host_from_id(id) {
            Ok(h) => h,
            Err(e) => {
                println!("[{id:?}] unavailable: {e}");
                continue;
            }
        };
        let init = t.elapsed();
        let din = h.default_input_device().map(|d| {
            (
                d.description().map(|x| x.to_string()).unwrap_or_default(),
                d.default_input_config().ok(),
            )
        });
        let dout = h.default_output_device().map(|d| {
            (
                d.description().map(|x| x.to_string()).unwrap_or_default(),
                d.default_output_config().ok(),
            )
        });
        let n_dev = h.devices().map(|d| d.count()).unwrap_or(0);
        println!("[{id:?}] init {} ms, {n_dev} devices", init.as_millis());
        if let Some((n, c)) = din {
            println!(
                "   default in : {n} :: {:?}",
                c.map(|c| (
                    c.sample_rate(),
                    c.channels(),
                    c.sample_format(),
                    c.buffer_size().clone()
                ))
            );
        }
        if let Some((n, c)) = dout {
            println!(
                "   default out: {n} :: {:?}",
                c.map(|c| (
                    c.sample_rate(),
                    c.channels(),
                    c.sample_format(),
                    c.buffer_size().clone()
                ))
            );
        }
    }
    let _ = HostId::Alsa;
}

fn read_wav_mono(path: &str) -> (Vec<f32>, u32) {
    let mut r = hound::WavReader::open(path).unwrap();
    let spec = r.spec();
    let ch = spec.channels as usize;
    let s: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|x| x.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let scale = (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|x| x.unwrap() as f32 / scale)
                .collect()
        }
    };
    (s.chunks(ch).map(|f| f[0]).collect(), spec.sample_rate)
}

fn write_wav(path: &str, s: &[f32], sr: u32) {
    // 16-bit PCM so sherpa-onnx's WAV reader (S3 `vadfile`) can read it.
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for x in s {
        w.write_sample((x.clamp(-1.0, 1.0) * 32767.0) as i16)
            .unwrap();
    }
    w.finalize().unwrap();
}

/// Whole-clip FFT resampling (offline helper); returns output and the
/// resampler's own delay in output frames (already trimmed by process_all).
fn resample_all(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return x.to_vec();
    }
    let mut r = Fft::<f32>::new(from as usize, to as usize, 1024, 1, FixedSync::Input).unwrap();
    let need = r.process_all_needed_output_len(x.len());
    let mut out = vec![0.0f32; need];
    let inp = InterleavedSlice::new(x, 1, x.len()).unwrap();
    let mut outa = InterleavedSlice::new_mut(&mut out, 1, need).unwrap();
    let (_, n) = r
        .process_all_into_buffer(&inp, &mut outa, x.len(), None)
        .unwrap();
    out.truncate(n);
    out
}

/// Plays `play.wav` (scaled by `gain`) on the default output while capturing the
/// default input; writes mic.wav / ref.wav (exact played mono signal) at the
/// device rate plus timing stats.
fn duplex(host_name: &str, play: &str, gain: f32, secs: f32, out_dir: &str) {
    let h = host(host_name);
    let ind = h.default_input_device().unwrap();
    let outd = h.default_output_device().unwrap();
    let icfg = ind.default_input_config().unwrap();
    let ocfg = outd.default_output_config().unwrap();
    println!("in : {} {:?}", ind.description().unwrap(), icfg);
    println!("out: {} {:?}", outd.description().unwrap(), ocfg);
    assert_eq!(icfg.sample_format(), SampleFormat::F32, "spike assumes f32");
    assert_eq!(ocfg.sample_format(), SampleFormat::F32, "spike assumes f32");
    let osr = ocfg.sample_rate();
    let isr = icfg.sample_rate();
    let och = ocfg.channels() as usize;
    let ich = icfg.channels() as usize;

    let (clip, csr) = read_wav_mono(play);
    let t = Instant::now();
    let mut clip = resample_all(&clip, csr, osr);
    println!(
        "clip {csr} Hz -> {osr} Hz in {} ms ({:.2}s audio)",
        t.elapsed().as_millis(),
        clip.len() as f32 / osr as f32
    );
    // 0.5 s of silence first so the AEC sees a clean start.
    let mut sig = vec![0.0f32; osr as usize / 2];
    sig.append(&mut clip);
    for x in &mut sig {
        *x *= gain;
    }
    let total_out = (secs * osr as f32) as usize;
    sig.resize(total_out, 0.0);

    // Lock-free rings: callbacks never allocate or lock.
    let (mut ref_tx, mut ref_rx) = rtrb::RingBuffer::<f32>::new(osr as usize * 4);
    let (mut mic_tx, mut mic_rx) = rtrb::RingBuffer::<f32>::new(isr as usize * 4);
    let sig = Arc::new(sig);
    let pos = Arc::new(AtomicU64::new(0));
    let first_play: Arc<std::sync::OnceLock<(Instant, StreamInstant, StreamInstant)>> =
        Arc::default();
    let first_cap: Arc<std::sync::OnceLock<(Instant, StreamInstant, StreamInstant)>> =
        Arc::default();
    let out_sizes = Arc::new(std::sync::Mutex::new(Vec::<usize>::new()));
    let in_sizes = Arc::new(std::sync::Mutex::new(Vec::<usize>::new()));
    let errors = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let (s2, p2, fp, os, e1) = (
        sig.clone(),
        pos.clone(),
        first_play.clone(),
        out_sizes.clone(),
        errors.clone(),
    );
    let ostream = outd
        .build_output_stream::<f32, _, _>(
            ocfg.config(),
            move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let ts = info.timestamp();
                let start = p2.load(Ordering::Relaxed) as usize;
                if start == 0 {
                    let _ = fp.set((Instant::now(), ts.callback, ts.playback));
                }
                let frames = data.len() / och;
                for f in 0..frames {
                    let v = s2.get(start + f).copied().unwrap_or(0.0);
                    for c in 0..och {
                        data[f * och + c] = v;
                    }
                    let _ = ref_tx.push(v);
                }
                p2.store((start + frames) as u64, Ordering::Relaxed);
                // try_lock: never block the audio thread (spike-only stats).
                if let Ok(mut v) = os.try_lock() {
                    v.push(frames);
                }
            },
            move |e| e1.lock().unwrap().push(format!("out: {e}")),
            None,
        )
        .unwrap();
    let (fc, is, e2) = (first_cap.clone(), in_sizes.clone(), errors.clone());
    let istream = ind
        .build_input_stream::<f32, _, _>(
            icfg.config(),
            move |data: &[f32], info: &cpal::InputCallbackInfo| {
                let ts = info.timestamp();
                let _ = fc.set((Instant::now(), ts.callback, ts.capture));
                for f in data.chunks(ich) {
                    let _ = mic_tx.push(f[0]);
                }
                if let Ok(mut v) = is.try_lock() {
                    v.push(data.len() / ich);
                }
            },
            move |e| e2.lock().unwrap().push(format!("in: {e}")),
            None,
        )
        .unwrap();
    let t0 = Instant::now();
    istream.play().unwrap();
    ostream.play().unwrap();
    let mut mic = Vec::new();
    let mut refr = Vec::new();
    while t0.elapsed() < Duration::from_secs_f32(secs) {
        std::thread::sleep(Duration::from_millis(20));
        while let Ok(x) = mic_rx.pop() {
            mic.push(x);
        }
        while let Ok(x) = ref_rx.pop() {
            refr.push(x);
        }
    }
    drop(ostream);
    drop(istream);
    while let Ok(x) = mic_rx.pop() {
        mic.push(x);
    }
    while let Ok(x) = ref_rx.pop() {
        refr.push(x);
    }
    let (pi, pcb, pplay) = *first_play.get().unwrap();
    let (ci, ccb, ccap) = *first_cap.get().unwrap();
    let os = out_sizes.lock().unwrap();
    let is = in_sizes.lock().unwrap();
    let stat = |v: &[usize]| {
        (
            v.iter().min().copied().unwrap_or(0),
            v.iter().max().copied().unwrap_or(0),
            v.len(),
        )
    };
    println!(
        "out callbacks (min,max,count) frames: {:?}  -> {:.1} ms buffers",
        stat(&os),
        stat(&os).1 as f32 * 1000.0 / osr as f32
    );
    println!(
        "in  callbacks (min,max,count) frames: {:?}  -> {:.1} ms buffers",
        stat(&is),
        stat(&is).1 as f32 * 1000.0 / isr as f32
    );
    println!(
        "out: playback - callback = {:.1} ms (reported output latency)",
        pplay.duration_since(pcb).as_secs_f64() * 1e3
    );
    println!(
        "in : callback - capture  = {:.1} ms (reported input latency)",
        ccb.duration_since(ccap).as_secs_f64() * 1e3
    );
    // Offset (in mic samples) at which ref sample 0 is expected in the mic,
    // from the stream timestamps (wall-clock instants as a cross-check).
    let ts_offset_ms = {
        let a = pplay.duration_since(StreamInstant::ZERO).as_secs_f64();
        let b = ccap.duration_since(StreamInstant::ZERO).as_secs_f64();
        (a - b) * 1e3
    };
    let wall_ms = pi.duration_since(ci.min(pi)).as_secs_f64() * 1e3
        - ci.duration_since(pi.min(ci)).as_secs_f64() * 1e3;
    println!(
        "timestamp offset playback0 - capture0 = {ts_offset_ms:.1} ms (wall-clock first-callback diff {wall_ms:.1} ms)"
    );
    println!("errors: {:?}", errors.lock().unwrap());
    println!(
        "captured {:.2}s mic @ {isr}, {:.2}s ref @ {osr}",
        mic.len() as f32 / isr as f32,
        refr.len() as f32 / osr as f32
    );
    std::fs::create_dir_all(out_dir).unwrap();
    write_wav(&format!("{out_dir}/mic.wav"), &mic, isr);
    write_wav(&format!("{out_dir}/ref.wav"), &refr, osr);
    std::fs::write(
        format!("{out_dir}/offset_ms.txt"),
        format!("{ts_offset_ms}\n"),
    )
    .unwrap();
}

fn goertzel_db(x: &[f32], sr: f32, f: f32) -> f32 {
    let w = 2.0 * std::f32::consts::PI * f / sr;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s = v + c * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    let p = s1 * s1 + s2 * s2 - c * s1 * s2;
    10.0 * (p.max(1e-20) / (x.len() as f32 * x.len() as f32 / 4.0)).log10()
}

/// rubato 5 FFT 48 kHz -> 16 kHz: passband level, aliasing rejection,
/// streaming latency and per-10 ms cost.
fn resample_test() {
    let sr = 48_000.0;
    let n = 48_000 * 2;
    for (f, label) in [
        (1000.0, "1 kHz passband"),
        (6000.0, "6 kHz passband"),
        (7600.0, "7.6 kHz edge"),
        (9000.0, "9 kHz -> alias 7 kHz"),
        (12000.0, "12 kHz -> alias 4 kHz"),
        (20000.0, "20 kHz -> alias 4 kHz"),
    ] {
        let x: Vec<f32> = (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * f * i as f32 / sr).sin())
            .collect();
        let y = resample_all(&x, 48_000, 16_000);
        let mid = &y[4000..y.len() - 4000];
        let alias = if f > 8000.0 {
            (16_000.0 - f % 16_000.0).abs().min(f % 16_000.0)
        } else {
            f
        };
        let lvl = goertzel_db(mid, 16_000.0, alias) - goertzel_db(&x[12000..n - 12000], sr, f);
        println!("resample {label:<22}: output at {alias:>5} Hz = {lvl:>7.1} dB relative to input");
    }
    // Streaming: 10 ms (480 frames) chunks, as from a capture callback.
    for chunk in [480usize, 1024] {
        let mut r = Fft::<f32>::new(48_000, 16_000, chunk, 1, FixedSync::Input).unwrap();
        let x = vec![0.1f32; chunk];
        let mut y = vec![0.0f32; r.output_frames_max()];
        let t = Instant::now();
        let iters = 2000;
        for _ in 0..iters {
            let inp = InterleavedSlice::new(&x, 1, chunk).unwrap();
            let mut out = InterleavedSlice::new_mut(&mut y, 1, r.output_frames_max()).unwrap();
            r.process_into_buffer(&inp, &mut out, None).unwrap();
        }
        let per = t.elapsed().as_secs_f64() * 1e6 / iters as f64;
        println!(
            "resample stream chunk {chunk} in: {per:.1} us/call ({:.2} us per 10 ms audio), output delay {} frames = {:.1} ms (+ chunk buffering {:.1} ms)",
            per * 480.0 / chunk as f64,
            r.output_delay(),
            r.output_delay() as f32 / 16.0,
            chunk as f32 / 48.0
        );
    }
}

fn rms_db(x: &[f32]) -> f32 {
    let p = x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32;
    10.0 * p.max(1e-12).log10()
}

/// Offline AEC on a duplex recording: 16 kHz mono, 10 ms frames.
fn aec(dir: &str, near: Option<(&str, f32, f32)>) {
    use sonora::config::EchoCanceller;
    use sonora::{AudioProcessing, Config, StreamConfig};
    let (mic, msr) = read_wav_mono(&format!("{dir}/mic.wav"));
    let (refr, rsr) = read_wav_mono(&format!("{dir}/ref.wav"));
    let t = Instant::now();
    let mut mic = resample_all(&mic, msr, 16_000);
    let mut refr = resample_all(&refr, rsr, 16_000);
    println!("resampled both to 16 kHz in {} ms", t.elapsed().as_millis());
    // Align roughly with the stream timestamps (ref sample 0 lands at offset in mic).
    let off_ms: f64 = std::fs::read_to_string(format!("{dir}/offset_ms.txt"))
        .map(|s| s.trim().parse().unwrap())
        .unwrap_or(0.0);
    let off = (off_ms * 16.0).round() as i64;
    if off > 0 {
        let mut pad = vec![0.0; off as usize];
        pad.append(&mut refr);
        refr = pad;
    } else if off < 0 {
        refr.drain(..((-off) as usize).min(refr.len()));
    }
    let n = mic.len().min(refr.len());
    mic.truncate(n);
    refr.truncate(n);
    // Optional synthetic echo path (S4_SYNTH_ERL_DB=<dB below ref>): the box's
    // USB mic does not pick up its speakers, so add ref * room impulse response
    // (60 ms bulk delay, RT60 ~ 300 ms, deterministic) and mild loudspeaker
    // nonlinearity on top of the real mic noise floor.
    if let Ok(erl) = std::env::var("S4_SYNTH_ERL_DB") {
        let erl: f32 = erl.parse().unwrap();
        let len = 16_000 * 3 / 10;
        let mut seed = 0x1234_5678u32;
        let mut rir = vec![0.0f32; 960 + len];
        rir[960] = 1.0;
        for k in 1..len {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let r = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            rir[960 + k] = r * 0.5 * (-6.9 * k as f32 / len as f32).exp();
        }
        let e: f32 = rir.iter().map(|v| v * v).sum::<f32>().sqrt();
        let g = 10f32.powf(-erl / 20.0) / e;
        let drive: Vec<f32> = refr.iter().map(|v| (1.5 * v).tanh() / 1.5).collect();
        let nz: Vec<usize> = (0..rir.len()).filter(|&k| rir[k].abs() > 1e-4).collect();
        for i in 0..n {
            let mut acc = 0.0f32;
            for &k in &nz {
                if k <= i {
                    acc += rir[k] * drive[i - k];
                }
            }
            mic[i] += g * acc;
        }
        println!(
            "synthetic echo path: ERL {erl} dB, bulk delay 60 ms, RT60 ~300 ms, tanh nonlinearity"
        );
    }
    // Measure residual delay by cross-correlation (0..300 ms).
    let mut best = (0usize, 0.0f32);
    for d in 0..4800 {
        let mut s = 0.0f32;
        for i in (d..n).step_by(4) {
            s += mic[i] * refr[i - d];
        }
        if s > best.1 {
            best = (d, s);
        }
    }
    println!(
        "acoustic + residual delay after timestamp alignment: {:.1} ms (xcorr peak)",
        best.0 as f32 / 16.0
    );

    let mut near_sig = vec![0.0f32; n];
    let mut near_range = None;
    if let Some((path, at, g)) = near {
        let (s, sr) = read_wav_mono(path);
        let s = resample_all(&s, sr, 16_000);
        let start = (at * 16_000.0) as usize;
        for (i, v) in s.iter().enumerate() {
            if start + i < n {
                near_sig[start + i] = v * g;
            }
        }
        let first = s.iter().position(|v| v.abs() > 0.02).unwrap_or(0);
        near_range = Some((start + first, (start + s.len()).min(n)));
        for i in 0..n {
            mic[i] += near_sig[i];
        }
        println!(
            "mixed near-end speech at {at:.2}s (onset {:.2}s), gain {g}",
            (start + first) as f32 / 16000.0
        );
    }

    let sc = StreamConfig::new(16_000, 1);
    let mut apm = AudioProcessing::builder()
        .config(Config {
            echo_canceller: Some(EchoCanceller::default()),
            ..Default::default()
        })
        .capture_config(sc)
        .render_config(sc)
        .build();
    let fl = sc.num_frames();
    let mut out = vec![0.0f32; n];
    let mut rbuf = vec![0.0f32; fl];
    let t = Instant::now();
    let mut stats = Vec::new();
    for (k, i) in (0..n - fl).step_by(fl).enumerate() {
        apm.process_render_f32(&[&refr[i..i + fl]], &mut [&mut rbuf])
            .unwrap();
        let (src, dst) = (&mic[i..i + fl], &mut out[i..i + fl]);
        apm.process_capture_f32(&[src], &mut [dst]).unwrap();
        if k % 100 == 99 {
            stats.push(apm.statistics().clone());
        }
    }
    let el = t.elapsed();
    println!(
        "sonora AEC3 16 kHz: {:.1} us per 10 ms frame ({:.3}% of one core)",
        el.as_secs_f64() * 1e6 / (n / fl) as f64,
        el.as_secs_f64() * 100.0 / (n as f64 / 16_000.0)
    );
    if let Some(s) = stats.last() {
        println!(
            "sonora stats (last): ERL {:?} dB, ERLE {:?} dB, delay {:?} ms, divergent {:?}",
            s.echo_return_loss,
            s.echo_return_loss_enhancement,
            s.delay_median_ms,
            s.divergent_filter_fraction
        );
    }
    // Own ERLE over 100 ms blocks where the reference is active and there is no near-end.
    let blk = 1600;
    let (mut pin, mut pout, mut nb) = (0.0f64, 0.0f64, 0);
    let mut series = Vec::new();
    for b in (16_000..n - blk).step_by(blk) {
        let r = rms_db(&refr[b..b + blk]);
        let in_near = near_range.is_some_and(|(s, e)| b + blk > s && b < e + 8000);
        if r > -40.0 && !in_near {
            let pi: f64 = mic[b..b + blk].iter().map(|v| (*v as f64).powi(2)).sum();
            let po: f64 = out[b..b + blk].iter().map(|v| (*v as f64).powi(2)).sum();
            pin += pi;
            pout += po;
            nb += 1;
            series.push(10.0 * (pi / po.max(1e-12)).log10());
        }
    }
    println!(
        "measured ERLE over {nb} far-end-only 100 ms blocks: {:.1} dB overall; per-block first {:?}",
        10.0 * (pin / pout.max(1e-12)).log10(),
        series
            .iter()
            .take(12)
            .map(|v| v.round() as i32)
            .collect::<Vec<_>>()
    );
    println!(
        "levels: mic {:.1} dBFS, ref {:.1} dBFS, aec out {:.1} dBFS",
        rms_db(&mic),
        rms_db(&refr),
        rms_db(&out)
    );
    if let Some((s, e)) = near_range {
        // How much of the near-end survives the AEC (double talk)?
        let pn: f64 = near_sig[s..e].iter().map(|v| (*v as f64).powi(2)).sum();
        let po: f64 = out[s..e].iter().map(|v| (*v as f64).powi(2)).sum();
        println!(
            "double-talk: AEC output / near-end energy over near-end span = {:.1} dB",
            10.0 * (po / pn.max(1e-12)).log10()
        );
    }
    write_wav(&format!("{dir}/mic16k.wav"), &mic, 16_000);
    write_wav(&format!("{dir}/ref16k.wav"), &refr, 16_000);
    write_wav(&format!("{dir}/aec16k.wav"), &out, 16_000);
    println!("wrote mic16k.wav, ref16k.wav, aec16k.wav (feed them to s3-sherpa-tts vadfile)");
}

/// Hot-plug simulation: stream on the default output while a null sink is
/// created, made default, and unloaded (pactl), with an error-driven rebuild.
fn hotplug(host_name: &str) {
    let h = host(host_name);
    let errors = Arc::new(std::sync::Mutex::new(Vec::<(f32, String)>::new()));
    let invalid = Arc::new(AtomicBool::new(false));
    let frames = Arc::new(AtomicU64::new(0));
    let t0 = Instant::now();
    let build = |dev: &cpal::Device| {
        let cfg = dev.default_output_config().unwrap();
        let (e, inv, fr) = (errors.clone(), invalid.clone(), frames.clone());
        let s = dev
            .build_output_stream::<f32, _, _>(
                cfg.config(),
                move |d: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    d.fill(0.0);
                    fr.fetch_add(d.len() as u64, Ordering::Relaxed);
                },
                move |err| {
                    if matches!(
                        err.kind(),
                        cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::StreamInvalidated
                    ) {
                        inv.store(true, Ordering::SeqCst);
                    }
                    e.lock().unwrap().push((
                        t0.elapsed().as_secs_f32(),
                        format!("{:?}: {err}", err.kind()),
                    ));
                },
                None,
            )
            .unwrap();
        s.play().unwrap();
        s
    };
    let sh = |c: &str| {
        let o = std::process::Command::new("sh")
            .arg("-c")
            .arg(c)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let orig = sh("pactl get-default-sink");
    let module = sh(
        "pactl load-module module-null-sink sink_name=s4_hotplug sink_properties=device.description=S4Hotplug",
    );
    std::thread::sleep(Duration::from_millis(500));
    // A: stream on the default device, default switches to the null sink and back.
    let mut a = build(&h.default_output_device().unwrap());
    sh("pactl set-default-sink s4_hotplug");
    std::thread::sleep(Duration::from_millis(800));
    sh(&format!("pactl set-default-sink {orig}"));
    std::thread::sleep(Duration::from_millis(800));
    println!("A (default follows): errors {:?}", errors.lock().unwrap());
    errors.lock().unwrap().clear();
    // B: stream opened on the null sink explicitly, then the sink disappears.
    // Fresh host: some backends snapshot the device list at host creation.
    let h2 = host(host_name);
    let names: Vec<String> = h2
        .output_devices()
        .unwrap()
        .map(|d| {
            format!(
                "{} [{}]",
                d.description().map(|x| x.to_string()).unwrap_or_default(),
                d.id().map(|i| i.to_string()).unwrap_or_default()
            )
        })
        .collect();
    println!("output devices on {host_name}: {names:?}");
    let dev = h2.output_devices().unwrap().find(|d| {
        let desc = d.description().map(|x| x.to_string()).unwrap_or_default();
        let id = d.id().map(|i| i.to_string()).unwrap_or_default();
        desc.contains("S4Hotplug") || desc.contains("s4_hotplug") || id.contains("s4_hotplug")
    });
    match dev {
        Some(d) => {
            a = build(&d);
            std::thread::sleep(Duration::from_millis(500));
            let tu = t0.elapsed().as_secs_f32();
            sh(&format!("pactl unload-module {module}"));
            let wait = Instant::now();
            while !invalid.load(Ordering::SeqCst) && wait.elapsed() < Duration::from_secs(3) {
                std::thread::sleep(Duration::from_millis(5));
            }
            let detected = invalid.load(Ordering::SeqCst);
            let f0 = frames.load(Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(500));
            println!(
                "B: after unplug the stream still pulls {} samples / 500 ms (callbacks alive = server re-routed it)",
                frames.load(Ordering::Relaxed) - f0
            );
            println!(
                "B (explicit device unplugged at {tu:.2}s): invalidation detected={detected} after {} ms; errors {:?}",
                wait.elapsed().as_millis(),
                errors.lock().unwrap()
            );
            let stalled = frames.load(Ordering::Relaxed) == f0 + 0 && !detected;
            if detected || stalled {
                // Watchdog path: no error, but no callbacks either -> rebuild.
                let f1 = frames.load(Ordering::Relaxed);
                let tr0 = Instant::now();
                drop(std::mem::replace(
                    &mut a,
                    build(&host(host_name).default_output_device().unwrap()),
                ));
                println!(
                    "B: watchdog rebuild on default device took {} ms",
                    tr0.elapsed().as_millis()
                );
                std::thread::sleep(Duration::from_millis(500));
                println!(
                    "B: after rebuild {} samples / 500 ms",
                    frames.load(Ordering::Relaxed) - f1
                );
            }
            if false {
                let tr = Instant::now();
                a = build(&h.default_output_device().unwrap());
                println!(
                    "B: rebuilt on default device in {} ms",
                    tr.elapsed().as_millis()
                );
            }
        }
        None => {
            println!("B: null sink not visible as a cpal device on {host_name}");
            sh(&format!("pactl unload-module {module}"));
        }
    }
    std::thread::sleep(Duration::from_millis(300));
    drop(a);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let arg = |i: usize| a.get(i).map(String::as_str);
    match arg(1) {
        Some("devices") => devices(),
        Some("duplex") => duplex(
            arg(2).unwrap(),
            arg(3).unwrap(),
            arg(4).unwrap().parse().unwrap(),
            arg(5).unwrap().parse().unwrap(),
            arg(6).unwrap(),
        ),
        Some("resample") => resample_test(),
        Some("aec") => {
            let near = arg(3).map(|p| {
                (
                    p,
                    arg(4).map_or(3.0, |s| s.parse().unwrap()),
                    arg(5).map_or(0.3, |s| s.parse().unwrap()),
                )
            });
            aec(arg(2).unwrap(), near)
        }
        Some("hotplug") => hotplug(arg(2).unwrap()),
        _ => eprintln!("see module docs for usage"),
    }
}
