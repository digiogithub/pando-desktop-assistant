//! Project selection (`PDA-SP-0008.R6`): fuzzy pre-filter to ≤ 12 candidates, then a
//! `project` question over the names plus `current`.
//!
//! The pre-filter is purely lexical and tolerant to speech-to-text noise: accents,
//! dashes spoken as spaces, split or merged words and one or two wrong letters. It
//! scores each name by the best character-trigram similarity between the name (with
//! and without separators) and every window of 1–3 transcript words. Embedding kNN
//! (Whistle / `needle_embed`, or the Pando KB) can replace [`score`] later through
//! [`prefilter_by`].

use crate::cascade::Answered;
use crate::error::DecisionError;
use crate::questions::{ProjectChoice, names, project_question};
use crate::state::{StateInput, build_state, state_byte_budget};
use crate::text::normalize;
use crate::transport::{DecisionTransport, SystemOneClient};

/// Lists longer than this are pre-filtered.
pub const PREFILTER_LIMIT: usize = crate::questions::MAX_PROJECT_CRITERIA;

type Grams = Vec<[char; 3]>;

/// Sorted, deduplicated character trigrams of `s` (padded so word edges count).
fn trigrams(s: &str) -> Grams {
    let padded: Vec<char> = format!("  {s} ").chars().collect();
    let mut g: Grams = padded.windows(3).map(|w| [w[0], w[1], w[2]]).collect();
    g.sort_unstable();
    g.dedup();
    g
}

/// Sørensen–Dice coefficient of two sorted trigram sets.
fn dice(a: &Grams, b: &Grams) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (mut i, mut j, mut inter) = (0, 0, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
        }
    }
    2.0 * inter as f64 / (a.len() + b.len()) as f64
}

/// A transcript prepared once for scoring many names: its normalized text and the
/// trigrams of every window of 1–3 words (spaced and joined).
#[derive(Debug, Clone)]
pub struct Scorer {
    padded: String,
    windows: Vec<Grams>,
}

impl Scorer {
    /// Prepare `transcript`.
    pub fn new(transcript: &str) -> Self {
        let t = normalize(transcript);
        let words: Vec<&str> = t.split(' ').filter(|w| !w.is_empty()).collect();
        let mut windows = Vec::new();
        for len in 1..=3.min(words.len()) {
            for w in words.windows(len) {
                windows.push(trigrams(&w.join(" ")));
                if len > 1 {
                    windows.push(trigrams(&w.concat()));
                }
            }
        }
        Self {
            padded: format!(" {t} "),
            windows,
        }
    }

    /// Similarity in `[0, 1]` with a name. A verbatim mention scores in `[0.99, 1]`,
    /// longer names higher, so "pando assistant" beats "pando" when both appear.
    pub fn score(&self, name: &str) -> f64 {
        let n = normalize(name);
        if n.is_empty() || self.windows.is_empty() {
            return 0.0;
        }
        if self.padded.contains(&format!(" {n} ")) {
            return 0.99 + 0.01 * (n.len().min(100) as f64 / 100.0);
        }
        let targets = [trigrams(&n), trigrams(&n.replace(' ', ""))];
        let mut best: f64 = 0.0;
        for w in &self.windows {
            for t in &targets {
                best = best.max(dice(w, t));
            }
        }
        best.min(0.99)
    }
}

/// Similarity in `[0, 1]` between a transcript and a candidate name (project or
/// agent); see [`Scorer`] to score many names against one transcript.
pub fn score(transcript: &str, name: &str) -> f64 {
    Scorer::new(transcript).score(name)
}

/// Up to `k` candidates ranked by `score_fn` (descending; ties keep the input order).
/// Lists of at most `k` are returned unchanged.
pub fn prefilter_by<F: Fn(&str) -> f64>(
    projects: &[String],
    k: usize,
    score_fn: F,
) -> Vec<&String> {
    if projects.len() <= k {
        return projects.iter().collect();
    }
    let mut scored: Vec<(f64, usize, &String)> = projects
        .iter()
        .enumerate()
        .map(|(i, p)| (score_fn(p), i, p))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(k).map(|(_, _, p)| p).collect()
}

/// Fuzzy pre-filter of `projects` for `transcript` to at most `k` names.
pub fn prefilter<'a>(transcript: &str, projects: &'a [String], k: usize) -> Vec<&'a String> {
    let scorer = Scorer::new(transcript);
    prefilter_by(projects, k, |p| scorer.score(p))
}

/// Ask the `project` question alone (idle pre-selection, "cambia a …"). The answer's
/// value and `p` feed the header pill (`proyecto · bitacora · 0.93`).
pub async fn select_project<T: DecisionTransport>(
    client: &SystemOneClient<T>,
    transcript: &str,
    projects: &[String],
    current: Option<&str>,
) -> Result<Answered<ProjectChoice>, DecisionError> {
    let candidates: Vec<String> = prefilter(transcript, projects, PREFILTER_LIMIT)
        .into_iter()
        .cloned()
        .collect();
    let questions = vec![(names::PROJECT.to_string(), project_question(&candidates))];
    let probe = client.request(String::new(), questions.clone());
    let qbytes = serde_json::to_vec(&probe).map_or(0, |b| b.len());
    let state = build_state(
        &StateInput {
            transcript: transcript.to_string(),
            agents: Vec::new(),
            projects: candidates,
            current_project: current.map(str::to_string),
        },
        state_byte_budget(client.options().context_tokens, qbytes),
    );
    let req = client.request(state, questions);
    let resp = client.decide(&req).await?.value;
    let a = resp
        .answers
        .get(names::PROJECT)
        .ok_or_else(|| DecisionError::Malformed("project answer missing".into()))?;
    Ok(Answered {
        value: ProjectChoice::from_key(a.choice.as_deref().unwrap_or_default()),
        p: a.p_top(),
        confidence: a.confidence,
        ranked: a.ranked(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::mock::{MockTransport, Reply};
    use crate::transport::{ClientOptions, block_on};

    /// 40 synthetic, deliberately confusable project names.
    fn projects() -> Vec<String> {
        [
            "bitacora",
            "bitacora-web",
            "pando",
            "pando-assistant",
            "pando-webui",
            "migracion-auth",
            "auth-service",
            "auth-gateway",
            "billing-api",
            "billing-worker",
            "facturacion",
            "facturas-legacy",
            "docs-site",
            "design-system",
            "mobile-app",
            "mobile-sdk",
            "data-pipeline",
            "data-lake",
            "ml-ranking",
            "ml-embeddings",
            "search-api",
            "search-indexer",
            "notificaciones",
            "notifier-email",
            "infra-terraform",
            "infra-k8s",
            "observabilidad",
            "metrics-exporter",
            "chat-widget",
            "chatbot-ventas",
            "inventario",
            "inventory-sync",
            "pagos-stripe",
            "pagos-redsys",
            "crm-integracion",
            "erp-connector",
            "portal-clientes",
            "portal-empleados",
            "whistle",
            "needle",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
    }

    /// How a user would say the name: dashes as spaces and Spanish accents.
    fn spoken(name: &str) -> String {
        name.replace('-', " ")
            .replace("bitacora", "bitácora")
            .replace("migracion", "migración")
            .replace("facturacion", "facturación")
            .replace("integracion", "integración")
    }

    /// Deterministic STT-like corruption: `kind` selects drop / swap / double / split /
    /// merge / substitute / none.
    fn corrupt(s: &str, kind: usize, seed: usize) -> String {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < 4 {
            return s.to_string();
        }
        let i = 1 + seed % (chars.len() - 2);
        let mut c = chars.clone();
        match kind % 7 {
            0 => {
                c.remove(i);
            }
            1 => c.swap(i, i + 1),
            2 => c.insert(i, chars[i]),
            3 => {
                if chars[i] != ' ' {
                    c.insert(i, ' ');
                }
            }
            4 => c.retain(|ch| *ch != ' '),
            5 => {
                c[i] = match chars[i] {
                    'b' => 'v',
                    'v' => 'b',
                    'c' => 'k',
                    's' => 'z',
                    'i' => 'y',
                    other => other,
                }
            }
            _ => {}
        }
        c.into_iter().collect()
    }

    const TEMPLATES: &[&str] = &[
        "cambia a {}",
        "cambia de proyecto a {}",
        "lanza un agente en {}",
        "abre el proyecto {}",
        "crea un agente de claude en el proyecto {} para arreglar los tests",
        "vamos a trabajar en {}",
        "{}",
        "pon {} como proyecto",
    ];

    /// Acceptance: the correct project is in the top 12 in ≥ 98 % of cases.
    #[test]
    fn correct_project_in_top_12_at_least_98_percent() {
        let projects = projects();
        assert_eq!(projects.len(), 40);
        let (mut hits, mut total) = (0usize, 0usize);
        let mut misses = Vec::new();
        for (pi, p) in projects.iter().enumerate() {
            for (ti, t) in TEMPLATES.iter().enumerate() {
                for kind in 0..7 {
                    let said = corrupt(&spoken(p), kind, pi * 31 + ti * 7 + kind);
                    let transcript = t.replace("{}", &said);
                    let top = prefilter(&transcript, &projects, PREFILTER_LIMIT);
                    total += 1;
                    if top.contains(&p) {
                        hits += 1;
                    } else {
                        misses.push(transcript);
                    }
                }
            }
        }
        let rate = hits as f64 / total as f64;
        println!("top-12 hit rate: {hits}/{total} = {rate:.4}");
        assert!(
            rate >= 0.98,
            "top-12 hit rate {rate:.4} ({hits}/{total}); misses: {misses:?}"
        );
    }

    #[test]
    fn exact_mentions_rank_first() {
        let projects = projects();
        let top = prefilter("cambia a pando assistant", &projects, 3);
        assert_eq!(top[0], "pando-assistant");
        let top = prefilter("lanza un agente en bitácora", &projects, 3);
        assert_eq!(top[0], "bitacora");
    }

    #[test]
    fn short_lists_are_untouched_and_scores_bounded() {
        let few: Vec<String> = vec!["b".into(), "a".into()];
        assert_eq!(prefilter("a", &few, 12), vec![&few[0], &few[1]]);
        assert_eq!(score("", "x"), 0.0);
        assert_eq!(score("x", ""), 0.0);
        for p in projects() {
            let s = score("cambia a bitácora web", &p);
            assert!((0.0..=1.0).contains(&s));
        }
    }

    /// Scenario PDA-SP-0008.R6 "switch": the pill shows `proyecto · bitacora · 0.93`.
    #[test]
    fn select_project_for_the_pill() {
        let c = SystemOneClient::new(
            MockTransport::new(vec![Reply::Answer(vec![("project", "bitacora", 0.93)])]),
            ClientOptions::new("tev1:0.8b"),
        );
        let a = block_on(select_project(
            &c,
            "cambia a bitácora",
            &projects(),
            Some("pando"),
        ))
        .unwrap();
        assert_eq!(a.value, ProjectChoice::Named("bitacora".into()));
        assert_eq!(a.p, Some(0.93));
        let sent = &c.transport().seen_json()[0];
        let keys = sent["questions"]["project"]["criteria"]
            .as_object()
            .unwrap();
        assert_eq!(keys.len(), PREFILTER_LIMIT + 1);
        assert!(keys.contains_key("bitacora") && keys.contains_key("current"));
    }
}
