//! The shared `state` string of a System One request, bounded by the model context.
//!
//! Like Pando's `modelrouter.BuildState`, the state is a JSON-object string. It must fit
//! the model context (`tev1:0.8b`: 2050 tokens) together with the questions, and the
//! whole body must stay under 64 KiB. When it does not fit, the builder drops projects
//! from the end, then agents from the end, then truncates the transcript, and sets
//! `"truncated": true`.

use serde::Serialize;

use crate::questions::AgentInfo;

/// Conservative bytes-per-token ratio for JSON with Spanish text. Over-estimating the
/// token count only trims the state earlier, which is the safe side.
pub const BYTES_PER_TOKEN: usize = 3;
/// Hard cap of the state itself, well under the 64 KiB body cap.
pub const MAX_STATE_BYTES: usize = 32 * 1024;
/// Tokens reserved for the model's own prompt template and output.
pub const RESERVED_TOKENS: usize = 128;

/// Inputs of one decision.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StateInput {
    /// The STT transcript.
    pub transcript: String,
    /// Live agents, most relevant first (the order decides what survives trimming).
    pub agents: Vec<AgentInfo>,
    /// Project names, most relevant first.
    pub projects: Vec<String>,
    /// Current project, if any.
    pub current_project: Option<String>,
}

#[derive(Serialize)]
struct StateJson<'a> {
    transcript: &'a str,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    agents: &'a [AgentInfo],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    projects: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    current_project: Option<&'a str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    truncated: bool,
}

/// Estimated tokens of a UTF-8 string.
pub fn estimate_tokens(bytes: usize) -> usize {
    bytes.div_ceil(BYTES_PER_TOKEN)
}

/// Byte budget left for the state given the model context and the encoded size of the
/// questions that travel with it.
pub fn state_byte_budget(context_tokens: usize, question_bytes: usize) -> usize {
    let left = context_tokens
        .saturating_sub(RESERVED_TOKENS)
        .saturating_sub(estimate_tokens(question_bytes));
    (left * BYTES_PER_TOKEN).min(MAX_STATE_BYTES)
}

/// Build the state string within `max_bytes` (never below what an empty transcript
/// needs). Deterministic: the same input and budget always give the same string.
pub fn build_state(input: &StateInput, max_bytes: usize) -> String {
    let render = |agents: &[AgentInfo], projects: &[String], transcript: &str, truncated| {
        serde_json::to_string(&StateJson {
            transcript,
            agents,
            projects,
            current_project: input.current_project.as_deref(),
            truncated,
        })
        .unwrap_or_else(|_| "{}".to_string())
    };

    let (mut na, mut np) = (input.agents.len(), input.projects.len());
    let full = render(&input.agents, &input.projects, &input.transcript, false);
    if full.len() <= max_bytes {
        return full;
    }
    while np > 0 {
        np -= 1;
        let s = render(
            &input.agents,
            &input.projects[..np],
            &input.transcript,
            true,
        );
        if s.len() <= max_bytes {
            return s;
        }
    }
    while na > 0 {
        na -= 1;
        let s = render(&input.agents[..na], &[], &input.transcript, true);
        if s.len() <= max_bytes {
            return s;
        }
    }
    // Truncate the transcript (keep the beginning, on a char boundary). JSON escaping
    // can expand it, so shrink until it fits.
    let base = render(&[], &[], "", true).len();
    let mut end = input.transcript.len().min(max_bytes.saturating_sub(base));
    loop {
        while !input.transcript.is_char_boundary(end) {
            end -= 1;
        }
        let s = render(&[], &[], &input.transcript[..end], true);
        if s.len() <= max_bytes || end == 0 {
            return s;
        }
        end = end.saturating_sub((s.len() - max_bytes).max(1));
    }
}

/// FNV-1a 64-bit hash of the state, hex-encoded, for the decision log.
pub fn state_hash(state: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in state.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn input(n_agents: usize, n_projects: usize, transcript: &str) -> StateInput {
        StateInput {
            transcript: transcript.into(),
            agents: (0..n_agents)
                .map(|i| AgentInfo {
                    id: format!("agente-{i}"),
                    name: format!("Agente número {i}"),
                    status: "running".into(),
                })
                .collect(),
            projects: (0..n_projects).map(|i| format!("proyecto-{i}")).collect(),
            current_project: Some("bitacora".into()),
        }
    }

    #[test]
    fn small_state_is_complete_and_valid_json() {
        let s = build_state(&input(2, 2, "cancela migración auth"), 4096);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["transcript"], "cancela migración auth");
        assert_eq!(v["agents"].as_array().unwrap().len(), 2);
        assert_eq!(v["projects"].as_array().unwrap().len(), 2);
        assert_eq!(v["current_project"], "bitacora");
        assert!(v.get("truncated").is_none());
    }

    #[test]
    fn trims_projects_before_agents_before_transcript() {
        let inp = input(10, 50, "cancela migración auth");
        let full = build_state(&inp, usize::MAX).len();
        let s = build_state(&inp, full - 1);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["agents"].as_array().unwrap().len(), 10);
        // The `truncated` flag costs a few bytes, so one or two projects go.
        assert!((47..50).contains(&v["projects"].as_array().unwrap().len()));
        assert_eq!(v["truncated"], true);

        let s = build_state(&inp, 300);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert!(v.get("projects").is_none());
        assert!(v["agents"].as_array().unwrap().len() < 10);
        assert_eq!(v["transcript"], "cancela migración auth");
    }

    #[test]
    fn tev1_budget_is_respected() {
        let budget = state_byte_budget(2050, 2000);
        assert!(budget < 2050 * BYTES_PER_TOKEN);
        let s = build_state(&input(24, 200, &"palabra ".repeat(2000)), budget);
        assert!(s.len() <= budget);
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(state_hash(""), "cbf29ce484222325");
        assert_eq!(state_hash("a"), "af63dc4c8601ec8c");
    }

    proptest! {
        #[test]
        fn always_valid_json_within_budget(
            transcript in "\\PC{0,400}",
            n_agents in 0usize..30,
            n_projects in 0usize..60,
            budget in 120usize..6000,
        ) {
            let s = build_state(&input(n_agents, n_projects, &transcript), budget);
            prop_assert!(s.len() <= budget, "len {} > {}", s.len(), budget);
            let v: serde_json::Value = serde_json::from_str(&s).unwrap();
            let t = v["transcript"].as_str().unwrap();
            prop_assert!(transcript.starts_with(t));
        }
    }
}
