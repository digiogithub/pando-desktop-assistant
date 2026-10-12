//! `wake-eval`: reproducible FAR / FRR / CPU evaluation of the hands-free wake path
//! (PDA-T-0141, `PDA-SP-0011.R3`). Runs the real pipeline (VAD -> Whistle verification -> fuzzy
//! match) offline over recorded or generated clips listed in a manifest. No audio is committed:
//! see `scripts/wake-manifest-from-spike.py` and `docs/wake-eval/README.md`.
//!
//! usage: wake-eval --manifest m.json --model whistle.cact [--vad energy|silero] [--vad-model f.onnx]
//!        [--sensitivity 0.4] [--lang es] [--subset N] [--out report.json] [--check]

use std::path::{Path, PathBuf};
use std::time::Instant;

use pando_stt::Whistle;
use pando_stt::handsfree::{HandsFree, HfEvent};
use pando_stt::stt::{Stt, SttOpts};
use pando_stt::vad::{EnergySegmenter, SpeechSegmenter};
use pando_stt::wake::{SttWakeVerifier, WakeConfig};
use serde_json::{Value, json};

struct Args {
    manifest: PathBuf,
    model: PathBuf,
    vad: String,
    vad_model: Option<PathBuf>,
    sensitivity: f32,
    lang: String,
    subset: Option<usize>,
    out: Option<PathBuf>,
    check: bool,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        manifest: PathBuf::new(),
        model: PathBuf::from("vendor/needle/models/whistle.cact"),
        vad: "energy".into(),
        vad_model: None,
        sensitivity: 0.4,
        lang: "es".into(),
        subset: None,
        out: None,
        check: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match k.as_str() {
            "--manifest" => a.manifest = val("--manifest")?.into(),
            "--model" => a.model = val("--model")?.into(),
            "--vad" => a.vad = val("--vad")?,
            "--vad-model" => a.vad_model = Some(val("--vad-model")?.into()),
            "--sensitivity" => {
                a.sensitivity = val("--sensitivity")?.parse().map_err(|e| format!("{e}"))?
            }
            "--lang" => a.lang = val("--lang")?,
            "--subset" => a.subset = Some(val("--subset")?.parse().map_err(|e| format!("{e}"))?),
            "--out" => a.out = Some(val("--out")?.into()),
            "--check" => a.check = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if a.manifest.as_os_str().is_empty() {
        return Err("--manifest is required".into());
    }
    Ok(a)
}

/// Process CPU seconds (Linux: /proc/self/stat utime+stime); wall seconds elsewhere.
fn cpu_seconds(t0: Instant) -> f64 {
    if let Ok(s) = std::fs::read_to_string("/proc/self/stat")
        && let Some(rest) = s.rsplit(')').next()
    {
        let f: Vec<&str> = rest.split_whitespace().collect();
        if let (Some(u), Some(k)) = (f.get(11), f.get(12))
            && let (Ok(u), Ok(k)) = (u.parse::<f64>(), k.parse::<f64>())
        {
            return (u + k) / 100.0; // clock ticks, 100 Hz on Linux
        }
    }
    t0.elapsed().as_secs_f64()
}

/// Stream a WAV (16 kHz mono, int16 or f32) through the pipeline in 100 ms blocks.
fn run_file(
    path: &Path,
    stt: &mut dyn Stt,
    mk: &dyn Fn() -> HandsFree,
    pad: bool,
) -> Result<(Vec<HfEvent>, f64, u64), String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = r.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 {
        return Err(format!("{}: need 16 kHz mono", path.display()));
    }
    let mut hf = mk();
    let mut events = Vec::new();
    let mut block: Vec<f32> = Vec::with_capacity(1600);
    let mut total: u64 = 0;
    let mut feed =
        |hf: &mut HandsFree, block: &mut Vec<f32>, stt: &mut dyn Stt| -> Result<(), String> {
            total += block.len() as u64;
            events.extend(hf.push(block, stt).map_err(|e| e.to_string())?);
            block.clear();
            Ok(())
        };
    if pad {
        for _ in 0..10 {
            block.resize(1600, 0.0);
            feed(&mut hf, &mut block, stt)?;
        }
    }
    let scale = (1u32 << (spec.bits_per_sample.max(1) - 1)) as f32;
    macro_rules! drive {
        ($iter:expr) => {
            for s in $iter {
                block.push(s);
                if block.len() == 1600 {
                    feed(&mut hf, &mut block, stt)?;
                }
            }
        };
    }
    match spec.sample_format {
        hound::SampleFormat::Float => drive!(r.samples::<f32>().map(|s| s.unwrap_or(0.0))),
        hound::SampleFormat::Int => {
            drive!(r.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / scale))
        }
    }
    if pad {
        for _ in 0..30 {
            block.resize(1600, 0.0);
            feed(&mut hf, &mut block, stt)?;
        }
    }
    if !block.is_empty() {
        feed(&mut hf, &mut block, stt)?;
    }
    events.extend(hf.finish(stt).map_err(|e| e.to_string())?);
    Ok((events, total as f64 / 16_000.0, hf.verifications))
}

fn wakes(ev: &[HfEvent]) -> usize {
    ev.iter()
        .filter(|e| matches!(e, HfEvent::Wake { .. }))
        .count()
}

fn entries<'a>(m: &'a Value, key: &str) -> Vec<&'a Value> {
    m.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn take_subset(subset: Option<usize>, v: Vec<&Value>) -> Vec<&Value> {
    match subset {
        Some(n) if v.len() > n && n > 0 => {
            let step = v.len() as f64 / n as f64;
            (0..n).map(|i| v[(i as f64 * step) as usize]).collect()
        }
        _ => v,
    }
}

fn path_of(v: &Value) -> PathBuf {
    v.get("path").and_then(Value::as_str).unwrap_or("").into()
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("wake-eval: {e}");
        std::process::exit(2);
    }
}

fn real_main() -> Result<(), String> {
    let a = args()?;
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&a.manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut stt = Whistle::load_file(&a.model).map_err(|e| e.to_string())?;
    let vad = a.vad.clone();
    let (sens, lang) = (a.sensitivity, a.lang.clone());
    let vad_model = a.vad_model.clone();
    let mk = move || -> HandsFree {
        let seg: Box<dyn SpeechSegmenter> = match (vad.as_str(), &vad_model) {
            #[cfg(feature = "silero-vad")]
            ("silero", Some(m)) => match pando_stt::vad::SileroSegmenter::new(m) {
                Ok(s) => Box::new(s),
                Err(e) => {
                    eprintln!("wake-eval: {e}; using the energy VAD");
                    Box::new(EnergySegmenter::default())
                }
            },
            _ => Box::new(EnergySegmenter::default()),
        };
        let cfg = WakeConfig::from_sensitivity(sens);
        HandsFree::new(
            seg,
            Box::new(SttWakeVerifier::new(cfg.clone(), Some(lang.clone()), &[])),
            cfg,
            SttOpts {
                lang: Some(lang.clone()),
                ..Default::default()
            },
        )
    };
    let subset = a.subset;

    // -- negatives: false accepts per hour, CPU
    let (mut neg_s, mut neg_wakes, mut neg_cpu, mut neg_verif) = (0.0, 0usize, 0.0, 0u64);
    for n in take_subset(subset, entries(&manifest, "negatives")) {
        let (t0, c0) = (Instant::now(), cpu_seconds(Instant::now()));
        let (ev, secs, v) = run_file(&path_of(n), &mut stt, &mk, false)?;
        neg_cpu += cpu_seconds(t0) - c0;
        neg_s += secs;
        neg_wakes += wakes(&ev);
        neg_verif += v;
    }
    // -- idle: CPU in a quiet room
    let (mut idle_s, mut idle_cpu, mut idle_wakes) = (0.0, 0.0, 0usize);
    for n in take_subset(subset, entries(&manifest, "idle")) {
        let (t0, c0) = (Instant::now(), cpu_seconds(Instant::now()));
        let (ev, secs, _) = run_file(&path_of(n), &mut stt, &mk, false)?;
        idle_cpu += cpu_seconds(t0) - c0;
        idle_s += secs;
        idle_wakes += wakes(&ev);
    }
    // -- positives: FRR per class and distance
    let mut groups: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for p in take_subset(subset, entries(&manifest, "positives")) {
        let class = p.get("class").and_then(Value::as_str).unwrap_or("oye");
        let dist = p
            .get("distance_m")
            .and_then(Value::as_f64)
            .map_or("?".into(), |d| format!("{d}"));
        let (ev, _, _) = run_file(&path_of(p), &mut stt, &mk, true)?;
        let e = groups.entry(format!("{class}@{dist}m")).or_default();
        e.0 += 1;
        if wakes(&ev) == 0 {
            e.1 += 1;
        }
    }
    // -- confusers
    let (mut conf_n, mut conf_fire) = (0usize, 0usize);
    for c in take_subset(subset, entries(&manifest, "confusers")) {
        let (ev, _, _) = run_file(&path_of(c), &mut stt, &mk, true)?;
        conf_n += 1;
        conf_fire += usize::from(wakes(&ev) > 0);
    }

    let hours = neg_s / 3600.0;
    let far = if hours > 0.0 {
        neg_wakes as f64 / hours
    } else {
        f64::NAN
    };
    let frr: serde_json::Map<String, Value> = groups
        .iter()
        .map(|(k, (n, miss))| {
            (
                k.clone(),
                json!({"clips": n, "missed": miss, "frr": *miss as f64 / *n as f64}),
            )
        })
        .collect();
    let frr_2m = groups
        .iter()
        .filter(|(k, _)| k.starts_with("oye@2"))
        .map(|(_, (n, m))| *m as f64 / *n as f64)
        .fold(0.0f64, f64::max);
    let idle_core = if idle_s > 0.0 {
        100.0 * idle_cpu / idle_s
    } else {
        f64::NAN
    };
    let neg_core = if neg_s > 0.0 {
        100.0 * neg_cpu / neg_s
    } else {
        f64::NAN
    };
    let report = json!({
        "vad": a.vad, "sensitivity": a.sensitivity, "subset": a.subset,
        "negatives": {"hours": hours, "false_accepts": neg_wakes, "far_per_hour": far, "verifications": neg_verif, "cpu_core_pct": neg_core},
        "idle": {"seconds": idle_s, "false_accepts": idle_wakes, "cpu_core_pct": idle_core},
        "frr": frr,
        "confusers": {"clips": conf_n, "fired": conf_fire},
        "targets": {"far_per_hour_lt": 1.0, "frr_2m_lt": 0.10, "idle_cpu_core_pct_lt": 3.0},
        "pass": {
            "far": far < 1.0 || far.is_nan(),
            "frr_2m": frr_2m < 0.10,
            "idle_cpu": idle_core < 3.0 || idle_core.is_nan(),
        },
    });
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    println!("{text}");
    if let Some(o) = &a.out {
        std::fs::write(o, &text).map_err(|e| e.to_string())?;
    }
    let ok = report["pass"]
        .as_object()
        .is_some_and(|m| m.values().all(|v| v.as_bool() == Some(true)));
    if a.check && !ok {
        std::process::exit(1);
    }
    Ok(())
}
