#![allow(clippy::expect_used)]
//! Needs the real engine and model; run with `cargo test -p pando-stt -- --ignored`.
//! Model path: `$PANDO_WHISTLE_MODEL` or `vendor/needle/models/whistle.cact` (scripts/fetch-needle.sh --model).
//! The engine is process-global, so everything is one test.

use pando_stt::{Stt, SttOpts, Whistle};

fn model() -> std::path::PathBuf {
    std::env::var_os("PANDO_WHISTLE_MODEL").map_or_else(
        || {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../vendor/needle/models/whistle.cact")
        },
        Into::into,
    )
}

fn wav() -> Vec<f32> {
    let p =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/oye_pando_es.wav");
    let mut r = hound::WavReader::open(p).expect("fixture");
    let spec = r.spec();
    assert_eq!((spec.sample_rate, spec.channels), (16_000, 1));
    let scale = (1u32 << (spec.bits_per_sample - 1)) as f32;
    r.samples::<i32>()
        .map(|s| s.expect("sample") as f32 / scale)
        .collect()
}

fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect()
}

#[test]
#[ignore = "needs libneedle and whistle.cact"]
fn spanish_fixture_is_transcribed() {
    let mut w = Whistle::load_file(&model()).expect("load model");
    assert!(matches!(
        Whistle::load(&[]),
        Err(pando_stt::SttError::AlreadyLoaded)
    ));
    let pcm = wav();
    let opts = SttOpts {
        lang: Some("es".into()),
        keywords: vec!["Pando".into(), "Bitácora".into()],
        word_ts: true,
    };
    let t = w.transcribe(&pcm, &opts).expect("transcribe");
    let n = norm(&t.text);
    assert!(n.contains("pando abre el proyecto"), "got {:?}", t.text);
    assert!(n.contains("proyecto"), "got {:?}", t.text);
    assert_eq!(t.lang, "es");
    assert!(!t.words.is_empty());
    // > 5 s input is split into chunks and joined
    let mut long = pcm.clone();
    long.extend(vec![0.0; 8_000]);
    long.extend(&pcm);
    long.extend(vec![0.0; 8_000]);
    long.extend(&pcm);
    let t3 = w.transcribe(&long, &opts).expect("long");
    assert_eq!(
        norm(&t3.text).matches("pando abre").count(),
        3,
        "{:?}",
        t3.text
    );
    // silence
    let s = w.transcribe(&vec![0.0; 16_000], &opts).expect("silence");
    assert!(s.text.is_empty());
}
