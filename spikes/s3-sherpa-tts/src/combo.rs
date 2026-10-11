//! Spike S3 (PDA-T-0020), part 2: sherpa-onnx (libstdc++, bundled ORT,
//! espeak-ng) and libneedle (clang + libc++ 21) linked into ONE binary.
//!
//! Exercises: both runtimes initialised, C++ exceptions thrown and caught on
//! both sides, concurrent use from two threads, and a TTS → STT round trip
//! (Piper / Kokoro speech transcribed by Whistle) as an intelligibility proxy.
//!
//! Usage: s3-combo <cache_dir>

use std::ffi::{CStr, CString, c_char, c_int, c_uchar, c_ulonglong};
use std::time::Instant;

use sherpa_onnx::{
    GenerationConfig, LinearResampler, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig, OfflineTtsVitsModelConfig,
};

unsafe extern "C" {
    fn needle_load(cact: *const c_uchar, n: c_ulonglong) -> c_int;
    fn needle_last_error() -> *const c_char;
    fn eh_shim_libstdcxx(n: c_int) -> c_int;
    fn needle_transcribe(
        pcm: *const f32,
        samples: c_int,
        language: *const c_char,
        keywords: *const c_char,
        word_timestamps: c_int,
        out: *mut c_char,
        out_capacity: c_int,
    ) -> c_int;
}

const PHRASES: &[&str] = &[
    "La tarea de migración auth ha terminado sin errores hoy.",
    "Oye Pando, cancela la migración del proyecto bitácora.",
    "Quedan dos avisos de estilo en el módulo de sesiones.",
    "¿Quieres que abra la pull request ahora?",
    "El agente de revisión necesita tu aprobación para continuar.",
];

fn last_err() -> String {
    unsafe {
        let p = needle_last_error();
        if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into()
        }
    }
}

fn transcribe(pcm16k: &[f32]) -> (String, u128) {
    let mut out = vec![0 as c_char; 64 * 1024];
    let lang = CString::new("es").unwrap();
    let kw = CString::new("Pando\nbitácora\nauth").unwrap();
    let t = Instant::now();
    let rc = unsafe {
        needle_transcribe(
            pcm16k.as_ptr(),
            pcm16k.len() as c_int,
            lang.as_ptr(),
            kw.as_ptr(),
            0,
            out.as_mut_ptr(),
            out.len() as c_int,
        )
    };
    let ms = t.elapsed().as_millis();
    if rc < 0 {
        return (format!("<error {rc}: {}>", last_err()), ms);
    }
    let s = unsafe { CStr::from_ptr(out.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    (s, ms)
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Word error rate by Levenshtein distance over words.
fn wer(reference: &str, hyp: &str) -> (usize, usize) {
    let r = words(reference);
    let h = words(hyp);
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut cur = vec![i; h.len() + 1];
        for j in 1..=h.len() {
            let sub = prev[j - 1] + usize::from(r[i - 1] != h[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    (prev[h.len()], r.len())
}

fn extract_text(json: &str) -> String {
    // The engine returns JSON; pull the "text" field without a JSON dep.
    json.split("\"text\"")
        .nth(1)
        .and_then(|s| s.split('"').nth(1))
        .unwrap_or(json)
        .to_string()
}

fn main() {
    let c = std::env::args().nth(1).expect("cache dir");

    // 0. libstdc++-built code throwing/catching under the mixed runtime.
    println!(
        "eh_shim_libstdcxx(1000) caught = {} (expect 2000)",
        unsafe { eh_shim_libstdcxx(1000) }
    );

    // 1. Exception path in libneedle (libc++): garbage model must fail cleanly.
    let junk = vec![0u8; 4096];
    let rc = unsafe { needle_load(junk.as_ptr(), junk.len() as c_ulonglong) };
    println!("needle_load(junk) = {rc} ({})", last_err());

    // 2. Exception path in sherpa/ORT (libstdc++): garbage ONNX file.
    let bad = format!("{c}/s3-bad.onnx");
    std::fs::write(&bad, b"not an onnx model").unwrap();
    let bad_cfg = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            vits: OfflineTtsVitsModelConfig {
                model: Some(bad.clone()),
                tokens: Some(format!("{c}/vits-piper-es_ES-davefx-medium/tokens.txt")),
                data_dir: Some(format!("{c}/vits-piper-es_ES-davefx-medium/espeak-ng-data")),
                ..Default::default()
            },
            num_threads: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    if std::env::var_os("S3_SKIP_BAD_ONNX").is_none() {
        println!(
            "sherpa create(bad onnx) is_some = {}",
            OfflineTts::create(&bad_cfg).is_some()
        );
    }

    // 3. Load Whistle for real.
    let cact = std::fs::read(format!("{c}/needle/whistle.cact")).unwrap();
    let t = Instant::now();
    let rc = unsafe { needle_load(cact.as_ptr(), cact.len() as c_ulonglong) };
    println!(
        "needle_load(whistle) = {rc} in {} ms ({})",
        t.elapsed().as_millis(),
        last_err()
    );
    assert!(rc >= 0);

    let piper = OfflineTts::create(&OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            vits: OfflineTtsVitsModelConfig {
                model: Some(format!(
                    "{c}/vits-piper-es_ES-davefx-medium/es_ES-davefx-medium.onnx"
                )),
                tokens: Some(format!("{c}/vits-piper-es_ES-davefx-medium/tokens.txt")),
                data_dir: Some(format!("{c}/vits-piper-es_ES-davefx-medium/espeak-ng-data")),
                ..Default::default()
            },
            num_threads: 2,
            ..Default::default()
        },
        ..Default::default()
    })
    .expect("piper");
    let kd = format!("{c}/kokoro-multi-lang-v1_0");
    let kokoro = OfflineTts::create(&OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            kokoro: OfflineTtsKokoroModelConfig {
                model: Some(format!("{kd}/model.onnx")),
                voices: Some(format!("{kd}/voices.bin")),
                tokens: Some(format!("{kd}/tokens.txt")),
                data_dir: Some(format!("{kd}/espeak-ng-data")),
                lexicon: Some(format!("{kd}/lexicon-us-en.txt,{kd}/lexicon-zh.txt")),
                dict_dir: Some(format!("{kd}/dict")),
                lang: Some("es".into()),
                ..Default::default()
            },
            num_threads: 4,
            ..Default::default()
        },
        ..Default::default()
    })
    .expect("kokoro");

    // 4. TTS → STT round trip, sequential.
    let voices: [(&str, &OfflineTts, i32); 3] = [
        ("piper davefx", &piper, 0),
        ("kokoro ef_dora", &kokoro, 28),
        ("kokoro em_alex", &kokoro, 29),
    ];
    for (name, tts, sid) in voices {
        let (mut errs, mut total) = (0, 0);
        for p in PHRASES {
            let a = tts
                .generate_with_config(
                    p,
                    &GenerationConfig {
                        sid,
                        ..Default::default()
                    },
                    None::<fn(&[f32], f32) -> bool>,
                )
                .unwrap();
            let rs = LinearResampler::create(a.sample_rate(), 16_000).unwrap();
            let pcm = rs.resample(a.samples(), true);
            let (json, ms) = transcribe(&pcm);
            let text = extract_text(&json);
            let (e, n) = wer(p, &text);
            errs += e;
            total += n;
            println!("  [{name}] {ms:>4} ms  WER {e}/{n}  «{text}»");
        }
        println!(
            "{name}: round-trip WER {:.1}% ({errs}/{total} words)",
            100.0 * errs as f32 / total as f32
        );
    }

    // 5. Concurrency: STT on one thread while TTS runs on another.
    let a = piper
        .generate_with_config(
            PHRASES[1],
            &GenerationConfig::default(),
            None::<fn(&[f32], f32) -> bool>,
        )
        .unwrap();
    let rs = LinearResampler::create(a.sample_rate(), 16_000).unwrap();
    let pcm = rs.resample(a.samples(), true);
    let t = Instant::now();
    std::thread::scope(|s| {
        let stt = s.spawn(|| (0..5).map(|_| transcribe(&pcm).1).collect::<Vec<_>>());
        let tts = s.spawn(|| {
            (0..10)
                .map(|_| {
                    let t = Instant::now();
                    piper
                        .generate_with_config(
                            PHRASES[0],
                            &GenerationConfig::default(),
                            None::<fn(&[f32], f32) -> bool>,
                        )
                        .unwrap();
                    t.elapsed().as_millis()
                })
                .collect::<Vec<_>>()
        });
        println!("concurrent: stt ms {:?}", stt.join().unwrap());
        println!("concurrent: tts ms {:?}", tts.join().unwrap());
    });
    println!("concurrent: wall {} ms; combo OK", t.elapsed().as_millis());
}
