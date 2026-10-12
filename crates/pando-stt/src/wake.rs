//! Wake phrase verification (PDA-T-0137, `PDA-SP-0011.R2`, spike S5 option A).
//!
//! A VAD segment of at most 2.5 s is transcribed with the keyword bias `Pando / Oye Pando / Hey Pando`;
//! the transcript is normalised and the first 1-2 words are fuzzy-matched (Levenshtein similarity)
//! against the wake phrases. The rest of the utterance is the command.

use crate::stt::{Stt, SttError, SttOpts};

/// Longest audio sent to the verification pass.
pub const VERIFY_SECONDS: f32 = 2.5;

/// Wake phrases (already normalised).
pub const DEFAULT_PHRASES: [&str; 3] = ["oye pando", "hey pando", "pando"];

/// Keyword bias for the verification pass.
pub const WAKE_BIAS: [&str; 3] = ["Pando", "Oye Pando", "Hey Pando"];

/// Matching options.
#[derive(Debug, Clone)]
pub struct WakeConfig {
    pub phrases: Vec<String>,
    /// Minimum similarity (0..=1) of the best window.
    pub threshold: f32,
}

impl WakeConfig {
    /// From a user sensitivity: 0.0 = exact phrase only, 0.4 = 0.9 (default), 1.0 = 0.75.
    pub fn from_sensitivity(sensitivity: f32) -> Self {
        let s = sensitivity.clamp(0.0, 1.0);
        Self {
            phrases: DEFAULT_PHRASES.iter().map(|p| p.to_string()).collect(),
            threshold: 1.0 - 0.25 * s,
        }
    }
}

impl Default for WakeConfig {
    fn default() -> Self {
        Self::from_sensitivity(0.4)
    }
}

/// Lowercase, accents removed, punctuation removed.
pub fn normalize_word(w: &str) -> String {
    w.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Edit distance between two strings (chars).
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// 1 - distance / longest length.
pub fn similarity(a: &str, b: &str) -> f32 {
    let m = a.chars().count().max(b.chars().count());
    if m == 0 {
        return 1.0;
    }
    1.0 - levenshtein(a, b) as f32 / m as f32
}

/// An accepted wake phrase.
#[derive(Debug, Clone, PartialEq)]
pub struct WakeMatch {
    pub score: f32,
    /// Text after the wake phrase, original spelling, leading punctuation trimmed.
    pub rest: String,
}

/// Fuzzy-match the first 1-2 words of `text` (allowing one filler word before the phrase).
pub fn match_wake(text: &str, cfg: &WakeConfig) -> Option<WakeMatch> {
    // (original token, normalised word) for tokens that have letters or digits
    let toks: Vec<(&str, String)> = text
        .split_whitespace()
        .map(|t| (t, normalize_word(t)))
        .filter(|(_, n)| !n.is_empty())
        .collect();
    let mut best: Option<(f32, usize, usize)> = None; // score, window len, end index
    for start in 0..toks.len().min(2) {
        for len in 1..=2usize {
            if start + len > toks.len() {
                continue;
            }
            let window = toks[start..start + len]
                .iter()
                .map(|(_, n)| n.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            for p in &cfg.phrases {
                if p.split(' ').count() != len {
                    continue;
                }
                let score = similarity(&window, p) - 0.02 * start as f32;
                let better = match best {
                    None => true,
                    Some((b, bl, _)) => score > b + 1e-6 || ((score - b).abs() <= 1e-6 && len > bl),
                };
                if better {
                    best = Some((score, len, start + len));
                }
            }
        }
    }
    let (score, _, end) = best?;
    if score < cfg.threshold {
        return None;
    }
    // original text after the matched tokens
    let consumed = &toks[end - 1].0;
    let off = text.find(consumed).map_or(0, |i| i + consumed.len());
    let rest = text[off..]
        .trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | '.' | ':' | ';' | '-' | '¿' | '¡')
        })
        .trim()
        .to_string();
    Some(WakeMatch { score, rest })
}

/// Result of verifying a segment.
#[derive(Debug, Clone, PartialEq)]
pub struct WakeHit {
    pub score: f32,
    /// Command text found in the same transcription (may be empty: "Oye Pando" alone).
    pub rest: String,
    /// Full transcript of the verification pass.
    pub text: String,
}

/// First-stage wake detector. The Whistle verifier is the v1 implementation; a trained
/// openWakeWord model (PDA-T-0142) can replace it without touching the caller.
pub trait WakeDetector {
    /// `segment`: 16 kHz mono audio of one VAD segment (any length; implementations look at the head).
    fn check(&mut self, segment: &[f32], stt: &mut dyn Stt) -> Result<Option<WakeHit>, SttError>;
}

/// VAD segment -> Whistle with the wake bias -> fuzzy match.
#[derive(Debug, Clone)]
pub struct SttWakeVerifier {
    pub cfg: WakeConfig,
    pub opts: SttOpts,
}

impl SttWakeVerifier {
    /// `lang`: forced language; `extra_keywords`: appended to the wake bias.
    pub fn new(cfg: WakeConfig, lang: Option<String>, extra_keywords: &[String]) -> Self {
        let mut keywords: Vec<String> = WAKE_BIAS.iter().map(|k| k.to_string()).collect();
        keywords.extend(extra_keywords.iter().cloned());
        Self {
            cfg,
            opts: SttOpts {
                lang,
                keywords,
                word_ts: false,
            },
        }
    }
}

impl WakeDetector for SttWakeVerifier {
    fn check(&mut self, segment: &[f32], stt: &mut dyn Stt) -> Result<Option<WakeHit>, SttError> {
        let n =
            ((VERIFY_SECONDS * voice_proto::STT_SAMPLE_RATE as f32) as usize).min(segment.len());
        let t = stt.transcribe(&segment[..n], &self.opts)?;
        Ok(match_wake(&t.text, &self.cfg).map(|m| WakeHit {
            score: m.score,
            rest: m.rest,
            text: t.text,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(t: &str) -> Option<WakeMatch> {
        match_wake(t, &WakeConfig::default())
    }

    #[test]
    fn spec_scenario_single_utterance() {
        let w = m("Oye Pando, cancela migración auth").expect("wake");
        assert_eq!(w.rest, "cancela migración auth");
        assert!(w.score > 0.99);
    }

    #[test]
    fn bare_pando_with_command() {
        assert_eq!(
            m("Pando, ¿qué hace bitácora?").expect("wake").rest,
            "qué hace bitácora?"
        );
    }

    #[test]
    fn wake_phrase_alone_has_empty_rest() {
        assert_eq!(m("Oye Pando.").expect("wake").rest, "");
        assert_eq!(m("hey pando").expect("wake").rest, "");
    }

    #[test]
    fn accents_and_case_are_ignored() {
        assert!(m("OYE PANDO abre").is_some());
        assert!(m("Óye Pándo, abre").is_some());
    }

    #[test]
    fn engine_mishearings_still_wake_through_the_pando_word() {
        // real Whistle output on a synthetic voice
        assert_eq!(
            m("Olle Pando abre el proyecto Bitácora.")
                .expect("wake")
                .rest,
            "abre el proyecto Bitácora."
        );
        assert!(m("Hoy Pando, abre").is_some());
    }

    #[test]
    fn confusers_are_rejected_at_default_sensitivity() {
        for t in [
            "Oye, panda.",
            "Oye Pablo, ven.",
            "Pan de molde.",
            "Hoy pagando la cuenta.",
            "Oye, cuándo vienes.",
            "hola qué tal",
            "",
            "   ",
        ] {
            assert!(m(t).is_none(), "{t:?} woke: {:?}", m(t));
        }
    }

    #[test]
    fn sensitivity_moves_the_threshold() {
        let strict = WakeConfig::from_sensitivity(0.0);
        let loose = WakeConfig::from_sensitivity(1.0);
        assert!(strict.threshold > WakeConfig::default().threshold);
        assert!(loose.threshold < WakeConfig::default().threshold);
        assert!(match_wake("oye panda", &strict).is_none());
        assert!(match_wake("oye panda", &loose).is_some());
        assert!(match_wake("oye pando", &strict).is_some());
    }

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("pando", "pando"), 0);
        assert_eq!(levenshtein("pando", "panda"), 1);
        assert_eq!(levenshtein("", "abc"), 3);
        assert!((similarity("oye pando", "oye panda") - 8.0 / 9.0).abs() < 1e-6);
    }

    #[test]
    fn rest_keeps_original_spelling_after_repeated_tokens() {
        let w = m("Pando pando dime la hora").expect("wake");
        assert_eq!(w.rest, "pando dime la hora");
    }

    struct Scripted(&'static str, usize);
    impl Stt for Scripted {
        fn name(&self) -> &'static str {
            "scripted"
        }
        fn transcribe(&mut self, pcm: &[f32], o: &SttOpts) -> Result<crate::Transcript, SttError> {
            self.1 = pcm.len();
            assert!(o.keywords.iter().any(|k| k == "Oye Pando"));
            Ok(crate::Transcript {
                text: self.0.into(),
                ..Default::default()
            })
        }
    }

    #[test]
    fn verifier_looks_only_at_the_first_two_and_a_half_seconds() {
        let mut v = SttWakeVerifier::new(
            WakeConfig::default(),
            Some("es".into()),
            &["Bitácora".into()],
        );
        let mut stt = Scripted("Oye Pando, abre bitácora", 0);
        let hit = v
            .check(&vec![0.0; 16_000 * 6], &mut stt)
            .expect("ok")
            .expect("hit");
        assert_eq!(stt.1, 40_000);
        assert_eq!(hit.rest, "abre bitácora");
        assert!(v.opts.keywords.contains(&"Bitácora".to_string()));
        let mut none = Scripted("buenas tardes", 0);
        assert!(v.check(&[0.0; 100], &mut none).expect("ok").is_none());
    }
}
