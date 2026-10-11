//! Spike S3 (PDA-T-0020): one statically linked sherpa-onnx 1.13.8 running
//! Silero VAD + Piper (VITS) + Kokoro in the same process.
//!
//! Usage:
//!   s3-sherpa-tts piper  <model_dir> [threads] [out.wav]
//!   s3-sherpa-tts kokoro <model_dir> <sid> [threads] [out.wav]
//!   s3-sherpa-tts vad    <silero.onnx> <piper_model_dir>
//!   s3-sherpa-tts all    <cache_dir>      # everything in one process
//!   s3-sherpa-tts vadfile <silero.onnx> <16k.wav> [threshold]  # S4 barge-in check
//!   s3-sherpa-tts bad    <cache_dir>      # garbage .onnx: does an ORT exception escape?
//!
//! Spike code: `unwrap`/`expect` are fine here, this never ships.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use sherpa_onnx::{
    GenerationConfig, LinearResampler, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig, OfflineTtsVitsModelConfig, SileroVadModelConfig, VadModelConfig,
    VoiceActivityDetector,
};

/// Ten-word Spanish sentence used for the time-to-first-audio criterion.
const TEN_WORDS: &str = "La tarea de migración auth ha terminado sin errores hoy.";
/// Longer, multi-sentence answer, to measure sentence-level streaming.
const PARAGRAPH: &str = "He revisado el proyecto. La migración de autenticación terminó \
    correctamente y los tests pasan. Quedan dos avisos de estilo en el módulo de sesiones, \
    nada bloqueante. ¿Quieres que abra la pull request ahora?";

fn piper(dir: &str, threads: i32) -> OfflineTts {
    let model = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "onnx"))
        .expect("no .onnx in piper dir");
    let cfg = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            vits: OfflineTtsVitsModelConfig {
                model: Some(model.to_string_lossy().into()),
                tokens: Some(format!("{dir}/tokens.txt")),
                data_dir: Some(format!("{dir}/espeak-ng-data")),
                ..Default::default()
            },
            num_threads: threads,
            provider: Some("cpu".into()),
            ..Default::default()
        },
        max_num_sentences: 1,
        ..Default::default()
    };
    OfflineTts::create(&cfg).expect("create piper")
}

fn kokoro(dir: &str, threads: i32) -> OfflineTts {
    let model = if std::path::Path::new(&format!("{dir}/model.int8.onnx")).exists() {
        format!("{dir}/model.int8.onnx")
    } else {
        format!("{dir}/model.onnx")
    };
    let cfg = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            kokoro: OfflineTtsKokoroModelConfig {
                model: Some(model),
                voices: Some(format!("{dir}/voices.bin")),
                tokens: Some(format!("{dir}/tokens.txt")),
                data_dir: Some(format!("{dir}/espeak-ng-data")),
                lexicon: Some(format!("{dir}/lexicon-us-en.txt,{dir}/lexicon-zh.txt")),
                dict_dir: Some(format!("{dir}/dict")),
                lang: Some("es".into()),
                ..Default::default()
            },
            num_threads: threads,
            provider: Some("cpu".into()),
            ..Default::default()
        },
        max_num_sentences: 1,
        ..Default::default()
    };
    OfflineTts::create(&cfg).expect("create kokoro")
}

struct Run {
    first_chunk: Duration,
    total: Duration,
    audio_secs: f32,
}

/// Synthesise `text`; the progress callback fires once per sentence
/// (max_num_sentences = 1), so its first call is the time to first audio.
fn synth(tts: &OfflineTts, text: &str, sid: i32, save: Option<&str>) -> Run {
    let (tx, rx) = mpsc::channel::<Instant>();
    let gen_cfg = GenerationConfig {
        sid,
        ..Default::default()
    };
    let t0 = Instant::now();
    let audio = tts
        .generate_with_config(
            text,
            &gen_cfg,
            Some(move |_s: &[f32], _p: f32| {
                let _ = tx.send(Instant::now());
                true
            }),
        )
        .expect("generate");
    let total = t0.elapsed();
    let first = rx.try_iter().next().map(|t| t - t0).unwrap_or(total);
    let audio_secs = audio.samples().len() as f32 / audio.sample_rate() as f32;
    if let Some(path) = save {
        audio.save(path);
    }
    Run {
        first_chunk: first,
        total,
        audio_secs,
    }
}

fn bench(name: &str, tts: &OfflineTts, sid: i32, out: Option<&str>) {
    // Warm-up: first inference pays ORT graph optimisation and espeak init.
    let cold = synth(tts, TEN_WORDS, sid, None);
    println!(
        "{name}: cold first-chunk {:>4} ms  total {:>4} ms  audio {:.2}s",
        cold.first_chunk.as_millis(),
        cold.total.as_millis(),
        cold.audio_secs
    );
    let mut firsts = Vec::new();
    let mut rtfs = Vec::new();
    for _ in 0..5 {
        let r = synth(tts, TEN_WORDS, sid, None);
        firsts.push(r.first_chunk.as_secs_f32() * 1000.0);
        rtfs.push(r.total.as_secs_f32() / r.audio_secs);
    }
    firsts.sort_by(f32::total_cmp);
    rtfs.sort_by(f32::total_cmp);
    println!(
        "{name}: 10-word warm first-chunk median {:.0} ms (min {:.0}, max {:.0})  RTF median {:.3}",
        firsts[2], firsts[0], firsts[4], rtfs[2]
    );
    let p = synth(tts, PARAGRAPH, sid, out);
    println!(
        "{name}: paragraph first-chunk {} ms  total {} ms  audio {:.2}s  RTF {:.3}",
        p.first_chunk.as_millis(),
        p.total.as_millis(),
        p.audio_secs,
        p.total.as_secs_f32() / p.audio_secs
    );
}

fn vad(model: &str, tts: &OfflineTts) {
    // Build a test clip: 1.0 s silence + TTS speech + 1.0 s silence, at 16 kHz.
    let audio = tts
        .generate_with_config(
            TEN_WORDS,
            &GenerationConfig::default(),
            None::<fn(&[f32], f32) -> bool>,
        )
        .unwrap();
    let rs = LinearResampler::create(audio.sample_rate(), 16_000).unwrap();
    let speech = rs.resample(audio.samples(), true);
    // Find the first sample above an energy floor to know the true onset.
    let lead = speech.iter().position(|s| s.abs() > 0.02).unwrap_or(0);
    let mut clip = vec![0.0f32; 16_000];
    clip.extend_from_slice(&speech);
    clip.extend(std::iter::repeat_n(0.0f32, 16_000));
    let true_onset = 16_000 + lead;
    let true_end = 16_000 + speech.len()
        - speech
            .iter()
            .rev()
            .position(|s| s.abs() > 0.02)
            .unwrap_or(0);

    let cfg = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(model.into()),
            threshold: 0.5,
            min_silence_duration: 0.25,
            min_speech_duration: 0.1,
            window_size: 512,
            max_speech_duration: 20.0,
        },
        sample_rate: 16_000,
        num_threads: 1,
        provider: Some("cpu".into()),
        debug: false,
        ..Default::default()
    };
    let t_load = Instant::now();
    let vad = VoiceActivityDetector::create(&cfg, 30.0).expect("create vad");
    println!("vad: load {} ms", t_load.elapsed().as_millis());

    let mut per_window = Vec::new();
    let mut onset_at: Option<usize> = None;
    let mut seg_end_at: Option<usize> = None;
    for (i, w) in clip.chunks(512).enumerate() {
        let t = Instant::now();
        vad.accept_waveform(w);
        per_window.push(t.elapsed().as_secs_f64() * 1e6);
        let fed = (i + 1) * 512;
        if onset_at.is_none() && vad.detected() {
            onset_at = Some(fed);
        }
        if seg_end_at.is_none() && !vad.is_empty() {
            seg_end_at = Some(fed);
        }
    }
    vad.flush();
    per_window.sort_by(f64::total_cmp);
    let n = per_window.len();
    println!(
        "vad: {} windows of 32 ms; per-window cost p50 {:.0} us  p95 {:.0} us  max {:.0} us",
        n,
        per_window[n / 2],
        per_window[n * 95 / 100],
        per_window[n - 1]
    );
    if let Some(o) = onset_at {
        println!(
            "vad: onset detected {:.0} ms after true onset (window granularity 32 ms)",
            (o as f64 - true_onset as f64) / 16.0
        );
    }
    if let Some(e) = seg_end_at {
        println!(
            "vad: segment closed {:.0} ms after true end (min_silence 250 ms)",
            (e as f64 - true_end as f64) / 16.0
        );
    }
    let mut segs = 0;
    while let Some(s) = vad.front() {
        segs += 1;
        println!(
            "vad: segment start {:.2}s len {:.2}s",
            s.start() as f32 / 16000.0,
            s.n() as f32 / 16000.0
        );
        vad.pop();
    }
    println!(
        "vad: {segs} segment(s); true speech {:.2}s..{:.2}s",
        true_onset as f32 / 16000.0,
        true_end as f32 / 16000.0
    );
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let arg = |i: usize| a.get(i).map(String::as_str);
    match arg(1) {
        Some("piper") => {
            let threads = arg(3).map_or(1, |s| s.parse().unwrap());
            let t = Instant::now();
            let tts = piper(arg(2).unwrap(), threads);
            println!(
                "piper: load {} ms, {} Hz, threads {threads}",
                t.elapsed().as_millis(),
                tts.sample_rate()
            );
            bench("piper", &tts, 0, arg(4));
        }
        Some("kokoro") => {
            let sid: i32 = arg(3).unwrap().parse().unwrap();
            let threads = arg(4).map_or(1, |s| s.parse().unwrap());
            let t = Instant::now();
            let tts = kokoro(arg(2).unwrap(), threads);
            println!(
                "kokoro: load {} ms, {} Hz, {} speakers, threads {threads}",
                t.elapsed().as_millis(),
                tts.sample_rate(),
                tts.num_speakers()
            );
            bench(&format!("kokoro sid {sid}"), &tts, sid, arg(5));
        }
        Some("vad") => {
            let tts = piper(arg(3).unwrap(), 1);
            vad(arg(2).unwrap(), &tts);
        }
        Some("all") => {
            // VAD + Piper + Kokoro live together in one process / one ORT.
            let c = arg(2).unwrap();
            let p = piper(&format!("{c}/vits-piper-es_ES-davefx-medium"), 2);
            let k = kokoro(&format!("{c}/kokoro-int8-multi-lang-v1_0"), 2);
            vad(&format!("{c}/silero_vad_v5.onnx"), &p);
            bench("all/piper", &p, 0, None);
            bench("all/kokoro ef_dora", &k, 28, None);
            println!("all: VAD + Piper + Kokoro coexisted in one process");
        }
        Some("vadfile") => {
            // Barge-in check for S4: Silero VAD over a 16 kHz WAV; report speech
            // segments and those long enough (>= 200 ms) to count as a barge-in.
            let wave = sherpa_onnx::Wave::read(arg(3).unwrap()).expect("read wav");
            assert_eq!(wave.sample_rate(), 16_000);
            let thr: f32 = arg(4).map_or(0.5, |s| s.parse().unwrap());
            let cfg = VadModelConfig {
                silero_vad: SileroVadModelConfig {
                    model: Some(arg(2).unwrap().into()),
                    threshold: thr,
                    min_silence_duration: 0.25,
                    min_speech_duration: 0.2,
                    window_size: 512,
                    max_speech_duration: 20.0,
                },
                sample_rate: 16_000,
                num_threads: 1,
                ..Default::default()
            };
            let vad = VoiceActivityDetector::create(&cfg, 60.0).unwrap();
            let mut first_detect = None;
            for (i, w) in wave.samples().chunks(512).enumerate() {
                vad.accept_waveform(w);
                if first_detect.is_none() && vad.detected() {
                    first_detect = Some((i + 1) as f32 * 512.0 / 16000.0);
                }
            }
            vad.flush();
            let mut n = 0;
            while let Some(s) = vad.front() {
                n += 1;
                println!(
                    "  segment {:.2}s +{:.2}s",
                    s.start() as f32 / 16000.0,
                    s.n() as f32 / 16000.0
                );
                vad.pop();
            }
            println!(
                "vadfile thr {thr}: {n} speech segment(s) >= 200 ms; first detection at {first_detect:?} s"
            );
        }
        Some("bad") => {
            // Garbage ONNX: does sherpa/ORT fail cleanly or let a C++ exception escape?
            let c = arg(2).unwrap();
            let bad = format!("{c}/s3-bad.onnx");
            std::fs::write(&bad, b"not an onnx model").unwrap();
            let cfg = OfflineTtsConfig {
                model: OfflineTtsModelConfig {
                    vits: OfflineTtsVitsModelConfig {
                        model: Some(bad),
                        tokens: Some(format!("{c}/vits-piper-es_ES-davefx-medium/tokens.txt")),
                        data_dir: Some(format!(
                            "{c}/vits-piper-es_ES-davefx-medium/espeak-ng-data"
                        )),
                        ..Default::default()
                    },
                    num_threads: 1,
                    ..Default::default()
                },
                ..Default::default()
            };
            println!(
                "create(bad onnx) is_some = {}",
                OfflineTts::create(&cfg).is_some()
            );
        }
        _ => eprintln!("see module docs for usage"),
    }
}
