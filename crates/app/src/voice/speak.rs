//! Choosing what to speak (PDA-T-0112, `PDA-SP-0005.R6`).
//!
//! Only the final answer of a voice-initiated run is spoken: the accumulated `content_delta`
//! until `done`. Tool calls, tool results, reasoning and plans are never spoken. A long answer
//! becomes its first paragraph plus "tienes el resto en pantalla". Delegation conclusions
//! (`summary` + `follow_up`) and the Claude Code Stop hook ("avísame") have their own phrasing.

use pando_client::chat::ChatEvent;
use pando_client::conclusion::{Conclusion, DelegatedResult, parse_delegated_results};
use rust_i18n::t;

/// Answers longer than this are cut to their first paragraph.
pub const MAX_SPOKEN_CHARS: usize = 420;

/// What feeding an event meant for the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fed {
    /// Nothing to act on.
    Nothing,
    /// The first non-empty `content_delta` arrived: Thinking ends here.
    FirstContent,
    /// The run finished.
    Done,
    /// The run failed.
    Failed,
}

/// Accumulates the answer of one run.
#[derive(Debug, Clone, Default)]
pub struct AnswerAccumulator {
    text: String,
    started: bool,
    session_id: Option<String>,
}

impl AnswerAccumulator {
    /// A fresh accumulator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one chat event. Everything except answer text is ignored.
    pub fn feed(&mut self, ev: &ChatEvent) -> Fed {
        match ev {
            ChatEvent::Session { session_id, .. } if !session_id.is_empty() => {
                self.session_id = Some(session_id.clone());
                Fed::Nothing
            }
            ChatEvent::ContentDelta(d) => {
                self.text.push_str(d);
                if !self.started && !self.text.trim().is_empty() {
                    self.started = true;
                    return Fed::FirstContent;
                }
                Fed::Nothing
            }
            // A steering prompt closes the current bubble: what follows is a new answer.
            ChatEvent::SteeringInjected(_) => {
                self.text.clear();
                self.started = false;
                Fed::Nothing
            }
            ChatEvent::Done => Fed::Done,
            ChatEvent::Error(_) => Fed::Failed,
            _ => Fed::Nothing,
        }
    }

    /// The session the run belongs to.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// The accumulated answer.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// What to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spoken {
    /// Text for the TTS (before `speech::normalize`).
    pub text: String,
    /// Part of the answer was left for the screen.
    pub truncated: bool,
}

/// Removes what sounds wrong aloud: fenced code, inline code ticks, headings, bullets, links.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let t = t
            .trim_start_matches('#')
            .trim_start_matches(['-', '*', '>'])
            .trim();
        let mut cleaned = String::new();
        let mut chars = t.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '`' | '*' | '_' => {}
                '[' => {
                    // [label](url) -> label
                    let mut label = String::new();
                    for c2 in chars.by_ref() {
                        if c2 == ']' {
                            break;
                        }
                        label.push(c2);
                    }
                    if chars.peek() == Some(&'(') {
                        for c2 in chars.by_ref() {
                            if c2 == ')' {
                                break;
                            }
                        }
                    }
                    cleaned.push_str(&label);
                }
                other => cleaned.push(other),
            }
        }
        out.push_str(&cleaned);
        out.push('\n');
    }
    out
}

/// The speakable form of a final answer; `None` when there is nothing to say (silent tool
/// work: the body goes Thinking → Idle).
#[must_use]
pub fn speakable(answer: &str) -> Option<Spoken> {
    let cleaned = plain(answer);
    let paragraphs: Vec<&str> = cleaned
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let first = paragraphs.first()?;
    let joined = paragraphs.join(" ").replace('\n', " ");
    if joined.chars().count() <= MAX_SPOKEN_CHARS && paragraphs.len() <= 2 {
        return Some(Spoken {
            text: joined,
            truncated: false,
        });
    }
    let mut head = first.replace('\n', " ");
    if head.chars().count() > MAX_SPOKEN_CHARS {
        head = cut_at_sentence(&head, MAX_SPOKEN_CHARS);
    }
    Some(Spoken {
        text: format!("{head} {}", t!("voice.speak.rest_on_screen")),
        truncated: true,
    })
}

fn cut_at_sentence(text: &str, max: usize) -> String {
    let prefix: String = text.chars().take(max).collect();
    match prefix.rfind(['.', '!', '?']) {
        Some(i) if i > max / 3 => prefix[..=i].to_owned(),
        _ => format!("{}…", prefix.trim_end()),
    }
}

/// Speech for a delegation conclusion.
#[must_use]
pub fn conclusion_speech(agent: &str, c: &Conclusion) -> String {
    let key = match c.status.as_str() {
        "failed" => "voice.speak.conclusion_failed",
        "blocked" => "voice.speak.conclusion_blocked",
        "partial" => "voice.speak.conclusion_partial",
        _ => "voice.speak.conclusion_done",
    };
    let mut out = t!(key, agent = agent).to_string();
    let summary = plain(&c.summary).trim().replace('\n', " ");
    if !summary.is_empty() {
        out.push(' ');
        out.push_str(&summary);
    }
    let follow = c.follow_up.trim();
    if !follow.is_empty() {
        out.push(' ');
        out.push_str(&t!("voice.speak.follow_up", text = follow));
    }
    out
}

/// Speech for each `[delegated-result ...]` block found in a piece of history.
#[must_use]
pub fn delegated_speech(history: &str) -> Vec<String> {
    parse_delegated_results(history)
        .iter()
        .map(|d: &DelegatedResult| {
            let c = Conclusion {
                status: d.status.clone(),
                summary: d.summary.clone(),
                follow_up: d.follow_up.clone(),
                ..Conclusion::default()
            };
            conclusion_speech(&d.task, &c)
        })
        .collect()
}

/// Speech for the Claude Code Stop hook when "avísame" was requested.
#[must_use]
pub fn claude_stop_speech(label: &str, last_message: &str) -> String {
    let body = speakable(last_message).map(|s| s.text).unwrap_or_default();
    let head = t!("voice.speak.claude_done", session = label).to_string();
    if body.is_empty() {
        head
    } else {
        format!("{head} {body}")
    }
}

#[cfg(test)]
mod tests {
    use pando_client::chat::{Note, ToolResultEvent};

    use super::*;

    fn es() {
        crate::i18n::apply(Some("es"));
    }

    #[test]
    fn only_content_deltas_are_accumulated_and_tool_output_is_never_spoken() {
        let mut a = AnswerAccumulator::new();
        assert_eq!(
            a.feed(&ChatEvent::ThinkingDelta("pienso".into())),
            Fed::Nothing
        );
        assert_eq!(
            a.feed(&ChatEvent::ToolResult(ToolResultEvent {
                content: "SECRET TOOL OUTPUT".into(),
                ..ToolResultEvent::default()
            })),
            Fed::Nothing
        );
        assert_eq!(
            a.feed(&ChatEvent::ContentDelta("  ".into())),
            Fed::Nothing,
            "blank is not content"
        );
        assert_eq!(
            a.feed(&ChatEvent::ContentDelta("Hola".into())),
            Fed::FirstContent
        );
        assert_eq!(
            a.feed(&ChatEvent::ContentDelta(" mundo".into())),
            Fed::Nothing
        );
        assert_eq!(a.feed(&ChatEvent::Done), Fed::Done);
        assert_eq!(a.text().trim(), "Hola mundo");
        assert!(!a.text().contains("SECRET"));
    }

    #[test]
    fn errors_and_steering_are_tracked() {
        let mut a = AnswerAccumulator::new();
        a.feed(&ChatEvent::ContentDelta("viejo".into()));
        a.feed(&ChatEvent::SteeringInjected(Note::default()));
        assert_eq!(a.text(), "");
        assert_eq!(
            a.feed(&ChatEvent::ContentDelta("nuevo".into())),
            Fed::FirstContent
        );
        assert_eq!(a.feed(&ChatEvent::Error("boom".into())), Fed::Failed);
    }

    #[test]
    fn a_short_answer_is_spoken_whole_without_markdown() {
        es();
        let s = speakable("Listo, **hecho**. Mira [el PR](http://x.y/1).").expect("speech");
        assert_eq!(s.text, "Listo, hecho. Mira el PR.");
        assert!(!s.truncated);
    }

    #[test]
    fn a_long_answer_becomes_the_first_paragraph_plus_a_pointer() {
        es();
        let long = format!("Primero esto.\n\n{}", "Detalle de relleno. ".repeat(60));
        let s = speakable(&long).expect("speech");
        assert!(s.truncated);
        assert!(s.text.starts_with("Primero esto."));
        assert!(
            s.text.ends_with("Tienes el resto en pantalla."),
            "{}",
            s.text
        );
        assert!(!s.text.contains("relleno"));
    }

    #[test]
    fn code_blocks_are_skipped_and_empty_answers_say_nothing() {
        es();
        let s = speakable("Cambié esto:\n```rust\nfn main() {}\n```\nY ya.").expect("speech");
        assert_eq!(s.text, "Cambié esto: Y ya.");
        assert_eq!(speakable("```\ncode\n```"), None);
        assert_eq!(speakable("   \n "), None);
    }

    #[test]
    fn a_conclusion_is_spoken_with_summary_and_follow_up() {
        es();
        let c = Conclusion {
            status: "success".into(),
            summary: "Migración aplicada.".into(),
            follow_up: "Revisar los índices".into(),
            ..Conclusion::default()
        };
        let s = conclusion_speech("migracion-auth", &c);
        assert!(s.contains("migracion-auth"));
        assert!(s.contains("Migración aplicada."));
        assert!(s.contains("Revisar los índices"));
        let failed = conclusion_speech(
            "x",
            &Conclusion {
                status: "failed".into(),
                ..Conclusion::default()
            },
        );
        assert_ne!(failed, conclusion_speech("x", &Conclusion::default()));
    }

    #[test]
    fn delegated_result_blocks_in_history_are_found() {
        es();
        let history = "[delegated-result task=build engine=pando project=p status=success confidence=0.9]\nsummary: Compila bien.\nfollow_up: Lanzar los tests\n";
        let lines = delegated_speech(history);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("Compila bien."));
    }

    #[test]
    fn the_claude_stop_hook_speaks_the_label_and_the_last_message() {
        es();
        let s = claude_stop_speech("api", "Terminé el refactor.");
        assert!(s.contains("api") && s.contains("Terminé el refactor."));
        assert!(claude_stop_speech("api", "").contains("api"));
    }
}
