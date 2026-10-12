#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Local STT quality gate (PDA-T-0096); NOT part of CI. Run it with `scripts/wer-gate.sh`.
//!
//! Transcribes the Spanish clips listed in `tests/fixtures/wer/manifest.tsv` (file, reference;
//! wavs live in `crates/pando-stt/tests/fixtures/`) through the real `pando-stt` worker and
//! fails when the corpus WER is more than 2 points worse than `baseline.json`.
//! `WER_UPDATE=1` rewrites the baseline. Needs `$PANDO_STT_MODEL` (Whistle `.cact`) and a built
//! `pando-stt` next to the test executables' parent (`cargo build -p pando-stt`).

use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use app::voice::wer::{gate, wer};
use voice_proto::{FromWorker, PROTOCOL_VERSION, ToWorker, read_msg, write_msg};

const TOLERANCE_POINTS: f64 = 2.0;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn worker() -> PathBuf {
    let exe = std::env::current_exe().expect("exe");
    exe.parent()
        .and_then(Path::parent)
        .expect("target dir")
        .join("pando-stt")
}

fn load_wav(path: &Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).expect("wav");
    let spec = r.spec();
    assert_eq!(
        (spec.sample_rate, spec.channels),
        (16_000, 1),
        "16 kHz mono expected"
    );
    let scale = (1u32 << (spec.bits_per_sample - 1)) as f32;
    r.samples::<i32>()
        .map(|s| s.expect("sample") as f32 / scale)
        .collect()
}

#[test]
#[ignore = "needs libneedle, whistle.cact ($PANDO_STT_MODEL) and a built pando-stt"]
fn wer_has_not_regressed() {
    let model = std::env::var("PANDO_STT_MODEL").expect("PANDO_STT_MODEL");
    let mut child = Command::new(worker())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn pando-stt (cargo build -p pando-stt)");
    let mut w = BufWriter::new(child.stdin.take().expect("stdin"));
    let mut r = BufReader::new(child.stdout.take().expect("stdout"));
    write_msg(
        &mut w,
        &ToWorker::Hello {
            version: PROTOCOL_VERSION,
        },
    )
    .expect("hello");
    assert!(matches!(
        read_msg::<_, FromWorker>(&mut r),
        Ok(Some(FromWorker::Hello { .. }))
    ));
    write_msg(&mut w, &ToWorker::LoadStt { model_path: model }).expect("load");
    assert!(matches!(
        read_msg::<_, FromWorker>(&mut r),
        Ok(Some(FromWorker::Ready))
    ));

    let manifest =
        std::fs::read_to_string(root().join("tests/fixtures/wer/manifest.tsv")).expect("manifest");
    let (mut errors, mut total) = (0.0_f64, 0.0_f64);
    for (id, line) in manifest
        .lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
    {
        let (file, reference) = line.split_once('\t').expect("file<TAB>reference");
        let pcm = load_wav(&root().join("../pando-stt/tests/fixtures").join(file));
        write_msg(
            &mut w,
            &ToWorker::Transcribe {
                id: id as u64,
                pcm,
                lang: Some("es".into()),
                keywords: vec![],
                word_ts: false,
            },
        )
        .expect("transcribe");
        let Ok(Some(FromWorker::Transcript { text, .. })) = read_msg::<_, FromWorker>(&mut r)
        else {
            panic!("no transcript for {file}");
        };
        let n = app::voice::wer::words(reference).len() as f64;
        let e = wer(reference, &text) * n;
        eprintln!(
            "{file}: {text:?} (WER {:.1} %)",
            wer(reference, &text) * 100.0
        );
        errors += e;
        total += n;
    }
    let _ = write_msg(&mut w, &ToWorker::Shutdown);
    drop(w);
    let _ = child.wait();
    let current = errors / total.max(1.0);
    let baseline_path = root().join("tests/fixtures/wer/baseline.json");
    if std::env::var_os("WER_UPDATE").is_some() || !baseline_path.exists() {
        std::fs::write(&baseline_path, format!("{{\"wer\": {current:.4}}}\n"))
            .expect("write baseline");
        eprintln!("baseline written: {:.1} %", current * 100.0);
        return;
    }
    let baseline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline_path).expect("baseline")).expect("json");
    let verdict = gate(
        baseline["wer"].as_f64().expect("wer"),
        current,
        TOLERANCE_POINTS,
    );
    assert!(
        verdict.passed,
        "WER regressed: baseline {:.1} % -> {:.1} % (tolerance {TOLERANCE_POINTS} points)",
        verdict.baseline_pct, verdict.current_pct
    );
}
