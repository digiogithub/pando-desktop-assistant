//! The typed questions of the cascade (`PDA-SP-0008.R1`) and their answer enums.
//!
//! Criteria order is part of the contract with the model: do not reorder the arrays
//! below without re-running the eval set (`decide-eval`). Instructions and descriptions
//! are model input, not UI copy, so they live here in English with Spanish product
//! examples.

use serde::{Deserialize, Serialize};

use crate::protocol::{Criterion, MAX_CHOICE_CRITERIA, Question};

/// Question names on the wire.
pub mod names {
    /// Intent question.
    pub const INTENT: &str = "intent";
    /// Target agent question.
    pub const AGENT: &str = "agent";
    /// Action question.
    pub const ACTION: &str = "action";
    /// Engine question (spawn).
    pub const ENGINE: &str = "engine";
    /// Project question (spawn, project switch).
    pub const PROJECT: &str = "project";
}

/// Criterion keys that are not agent ids / project names.
pub mod reserved {
    /// Every agent.
    pub const ALL: &str = "all";
    /// No agent.
    pub const NONE: &str = "none";
    /// The current project.
    pub const CURRENT: &str = "current";
}

macro_rules! keyed_enum {
    ($(#[$m:meta])* $name:ident { $($(#[$vm:meta])* $var:ident = $key:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum $name { $($(#[$vm])* #[serde(rename = $key)] $var),+ }

        impl $name {
            /// Every value, in criteria order.
            pub const ALL: &'static [$name] = &[$($name::$var),+];

            /// Wire key.
            pub fn key(self) -> &'static str {
                match self { $($name::$var => $key),+ }
            }

            /// Parse a wire key.
            pub fn from_key(k: &str) -> Option<Self> {
                match k { $($key => Some($name::$var),)+ _ => None }
            }
        }
    };
}

keyed_enum!(
    /// Step 1: what the user wants.
    Intent {
        /// A command for agents, executed in code.
        Control = "control",
        /// Talk to the orchestrator (chat stream).
        Conversation = "conversation",
        /// Text to insert into the composer without sending.
        Dictation = "dictation",
    }
);

keyed_enum!(
    /// Step 3: the command.
    Action {
        /// Pause an agent (no HTTP endpoint until gap G4).
        Pause = "pause",
        /// Resume an agent (no HTTP endpoint until gap G4).
        Resume = "resume",
        /// Cancel an agent. Destructive.
        Cancel = "cancel",
        /// Speak the status of an agent.
        Status = "status",
        /// Approve a pending permission/question.
        Approve = "approve",
        /// Deny a pending permission/question. Destructive.
        Deny = "deny",
        /// Relaunch an agent.
        Retry = "retry",
        /// Select an agent's cell.
        Open = "open",
        /// Launch a new agent.
        Spawn = "spawn",
    }
);

keyed_enum!(
    /// Engine for `spawn`.
    Engine {
        /// Pando's native agent.
        Pando = "pando",
        /// Claude Code.
        ClaudeCode = "claude-code",
        /// A generic CLI agent.
        Cli = "cli",
    }
);

impl Action {
    /// Reversible actions may execute in the 0.6–0.85 band (`PDA-SP-0008.R2`).
    pub fn is_reversible(self) -> bool {
        matches!(self, Self::Pause | Self::Status | Self::Open)
    }

    /// Destructive actions need p ≥ 0.95 **and** the literal agent id, else confirm.
    pub fn is_destructive(self) -> bool {
        matches!(self, Self::Cancel | Self::Deny)
    }

    /// Whether the action acts on one existing agent.
    pub fn needs_agent(self) -> bool {
        !matches!(self, Self::Spawn)
    }
}

/// Answer of the agent question.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTarget {
    /// One agent, by id.
    Agent(String),
    /// Every agent.
    All,
    /// No agent named.
    None,
}

impl AgentTarget {
    /// Parse a criterion key of the agent question.
    pub fn from_key(k: &str) -> Self {
        match k {
            reserved::ALL => Self::All,
            reserved::NONE => Self::None,
            id => Self::Agent(id.to_string()),
        }
    }
}

/// Answer of the project question.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectChoice {
    /// A project by name.
    Named(String),
    /// Keep the current project.
    Current,
}

impl ProjectChoice {
    /// Parse a criterion key of the project question.
    pub fn from_key(k: &str) -> Self {
        if k == reserved::CURRENT {
            Self::Current
        } else {
            Self::Named(k.to_string())
        }
    }
}

/// A live agent as shown to the model.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Stable id (e.g. `migracion-auth`); used as criterion key.
    pub id: String,
    /// Display name, used as criterion description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Status word (`running`, `waiting`, ...).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
}

/// Maximum agents in the agent question (26 criteria minus `all` and `none`).
pub const MAX_AGENT_CRITERIA: usize = MAX_CHOICE_CRITERIA - 2;
/// Maximum projects in the project question (`PDA-SP-0008.R6`), plus `current`.
pub const MAX_PROJECT_CRITERIA: usize = 12;

/// Fine-grained classes of the intent question, each mapped to an [`Intent`].
///
/// `tev1:0.8b` separates "command vs. request" much better when every kind of command
/// has its own criterion than with three abstract options (66 % → 97 % on the labelled
/// set in `tests/fixtures/intent_phrases.tsv`). The cascade sums the probabilities per
/// intent, so the policy still sees one `p(intent)`.
pub const INTENT_CLASSES: &[(&str, Intent, &str)] = &[
    (
        "stop_agent",
        Intent::Control,
        "stop, cancel or kill an agent or task (cancela, para, detén, mata)",
    ),
    (
        "agent_status",
        Intent::Control,
        "ask how an agent is going (¿cómo va?, ¿qué tal va?, estado de)",
    ),
    (
        "answer_permission",
        Intent::Control,
        "approve or deny a pending request (aprueba, deniega, permítelo, no lo permitas)",
    ),
    (
        "rerun_agent",
        Intent::Control,
        "retry or relaunch an agent (reintenta, relanza, vuelve a lanzar)",
    ),
    (
        "show_agent",
        Intent::Control,
        "open or show an agent (abre, enséñame, muéstrame)",
    ),
    (
        "pause_resume",
        Intent::Control,
        "pause or resume an agent (pausa, reanuda, continúa)",
    ),
    (
        "new_agent",
        Intent::Control,
        "launch a new agent (lanza, crea, arranca un agente)",
    ),
    (
        "switch_project",
        Intent::Control,
        "switch project (cambia a, cambia de proyecto)",
    ),
    (
        "question",
        Intent::Conversation,
        "ask the assistant a question (¿qué?, ¿por qué?, ¿dónde?, explícame)",
    ),
    (
        "work_request",
        Intent::Conversation,
        "ask the assistant to do work (resume, redacta, analiza, revisa, genera, traduce, refactoriza, propón)",
    ),
    ("chat", Intent::Conversation, "small talk (hola, gracias)"),
    (
        "dictate",
        Intent::Dictation,
        "dictated text to type literally (dicta, escribe:, apunta, toma nota, pon en el cuadro)",
    ),
];

/// Map a wire key of the intent question to its intent.
pub fn intent_of_class(key: &str) -> Option<Intent> {
    INTENT_CLASSES
        .iter()
        .find_map(|(k, i, _)| (*k == key).then_some(*i))
}

/// Step 1: intent, over [`INTENT_CLASSES`].
pub fn intent_question() -> Question {
    Question::choice(
        "What does the user want?",
        INTENT_CLASSES
            .iter()
            .map(|(k, _, d)| Criterion::new(*k, *d))
            .collect(),
    )
}

/// Spoken lead-ins that make a transcript dictation by rule, before any model call
/// (`PDA-SP-0008.R7`). The dictated content is itself an instruction ("dicta: añade un
/// test"), which a small classifier confuses with a request; an explicit lead word is
/// unambiguous and costs no round trip. Matched on the normalized transcript
/// (lowercase, no accents, punctuation as spaces), longest first.
const DICTATION_TRIGGERS: &[&str] = &[
    "dicta el siguiente texto",
    "escribe en el cuadro de texto",
    "pon en el cuadro de texto",
    "escribe en el prompt",
    "escribe en el agente",
    "escribe sin enviar",
    "pon en el cuadro",
    "escribe esto",
    "toma nota",
    "dictado",
    "apunta",
    "dicta",
];

/// If the transcript starts with a dictation lead-in, return the dictated text (the
/// original transcript after the lead-in, without leading punctuation). Bare
/// "escribe ..." is **not** a trigger ("escribe un plan" is a request); "escribe:" with
/// a colon is.
pub fn dictation_trigger(transcript: &str) -> Option<String> {
    let trimmed = transcript.trim_start();
    let norm = crate::text::normalize(trimmed);
    let words: Vec<&str> = norm.split(' ').filter(|w| !w.is_empty()).collect();
    let colon_escribe = trimmed
        .to_lowercase()
        .strip_prefix("escribe")
        .is_some_and(|r| r.trim_start().starts_with(':'));
    let n = if colon_escribe {
        1
    } else {
        DICTATION_TRIGGERS.iter().find_map(|t| {
            let tw: Vec<&str> = t.split(' ').collect();
            words.starts_with(&tw).then_some(tw.len())
        })?
    };
    // Skip `n` words of the original text, then separators.
    let mut rest = trimmed;
    for _ in 0..n {
        rest = rest.trim_start_matches(|c: char| !c.is_alphanumeric());
        let end = rest
            .find(|c: char| !c.is_alphanumeric())
            .unwrap_or(rest.len());
        rest = &rest[end..];
    }
    let rest = rest.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, ':' | ',' | '.' | '-' | ';')
    });
    Some(rest.trim_end().to_string())
}

/// A voice-target command recognised by rule (`PDA-SP-0008.R8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetTrigger {
    /// "háblale a <agente>": the agent question follows.
    Talk,
    /// "vuelve": back to the primary session; nothing else to ask.
    Back,
}

/// Lead-ins of "háblale a <agente>", matched on the normalized transcript, longest first.
/// The agent name must follow.
const TALK_TRIGGERS: &[&str] = &[
    "quiero hablar con",
    "pasame con",
    "ponme con",
    "hablale al",
    "hablale a",
    "habla con",
    "hablar con",
];

/// Whole utterances (after fillers) that mean "back to the primary session".
const BACK_PHRASES: &[&str] = &[
    "vuelve",
    "volver",
    "vuelve a pando",
    "vuelve con pando",
    "volver a pando",
    "vuelve a mesnada",
    "vuelve a la sesion principal",
    "vuelve al principal",
    "regresa a pando",
];

/// Words dropped from both ends before matching a back phrase ("ya está, vuelve a Pando",
/// "Pando, vuelve", "vuelve por favor").
const FILLERS: &[&str] = &[
    "ya", "esta", "vale", "bueno", "venga", "ok", "oye", "pando", "por", "favor", "y",
];

/// "háblale a auth" / "vuelve" by rule, before any model call (`PDA-SP-0008.R8`).
///
/// The 0.8b decision model got worse when these became intent classes: two extra
/// control classes pulled conversation requests to `control` (live eval 96/100 → 88/100)
/// and confused "volver" with "vuelve a lanzar". A short lead-in list is unambiguous and
/// costs no round trip, like the dictation lead-ins. "vuelve a lanzar ..." is not a back
/// phrase: back phrases must be the whole utterance.
pub fn target_trigger(transcript: &str) -> Option<TargetTrigger> {
    let norm = crate::text::normalize(transcript);
    let words: Vec<&str> = norm.split(' ').filter(|w| !w.is_empty()).collect();
    let talk = TALK_TRIGGERS.iter().any(|t| {
        let tw: Vec<&str> = t.split(' ').collect();
        words.len() > tw.len() && words.starts_with(&tw)
    });
    if talk {
        return Some(TargetTrigger::Talk);
    }
    let mut core: &[&str] = &words;
    while let [first, rest @ ..] = core
        && FILLERS.contains(first)
    {
        core = rest;
    }
    while let [rest @ .., last] = core
        && FILLERS.contains(last)
        // Keep the "pando" of "vuelve a pando".
        && !(core.len() >= 2 && *last == "pando" && matches!(core[core.len() - 2], "a" | "con"))
    {
        core = rest;
    }
    let core = core.join(" ");
    BACK_PHRASES
        .contains(&core.as_str())
        .then_some(TargetTrigger::Back)
}

/// Step 2: target agent over the given agents (first [`MAX_AGENT_CRITERIA`] with a
/// non-reserved, unique id) plus `all | none`.
pub fn agent_question(agents: &[AgentInfo]) -> Question {
    let mut seen = std::collections::BTreeSet::new();
    let mut criteria: Vec<Criterion> = agents
        .iter()
        .filter(|a| {
            !a.id.is_empty()
                && a.id != reserved::ALL
                && a.id != reserved::NONE
                && seen.insert(a.id.as_str())
        })
        .take(MAX_AGENT_CRITERIA)
        .map(|a| Criterion::new(a.id.clone(), a.name.clone()))
        .collect();
    criteria.push(Criterion::new(reserved::ALL, "Every agent ('todos')."));
    criteria.push(Criterion::new(
        reserved::NONE,
        "No specific agent is named or implied.",
    ));
    Question::choice(
        "Which agent does the command refer to? Match spoken names loosely: ids are \
         spoken with spaces instead of dashes and without accents.",
        criteria,
    )
}

/// Step 3.
pub fn action_question() -> Question {
    let d = |a: Action| match a {
        Action::Pause => "Pause the agent ('pausa', 'para un momento').",
        Action::Resume => "Resume a paused agent ('reanuda', 'continúa').",
        Action::Cancel => "Cancel / stop / kill the agent ('cancela', 'detén', 'mata').",
        Action::Status => "Ask how the agent is doing ('¿cómo va?', 'estado').",
        Action::Approve => {
            "Approve the pending permission or question ('aprueba', 'sí, adelante')."
        }
        Action::Deny => "Deny the pending permission or question ('deniega', 'no lo permitas').",
        Action::Retry => "Run the agent's task again ('reintenta', 'relanza').",
        Action::Open => "Show or focus the agent ('abre', 'enséñame').",
        Action::Spawn => "Launch a new agent ('lanza', 'crea un agente', 'arranca').",
    };
    Question::choice(
        "Which command does the user give?",
        Action::ALL
            .iter()
            .map(|a| Criterion::new(a.key(), d(*a)))
            .collect(),
    )
}

/// Spawn: engine.
pub fn engine_question() -> Question {
    Question::choice(
        "Which engine should run the new agent? Default to pando when none is named.",
        vec![
            Criterion::new(Engine::Pando.key(), "Pando's own agent (default)."),
            Criterion::new(
                Engine::ClaudeCode.key(),
                "Claude Code ('claude', 'cloud code').",
            ),
            Criterion::new(Engine::Cli.key(), "Another command-line agent ('CLI')."),
        ],
    )
}

/// Spawn / project switch: project over (pre-filtered) names plus `current`.
pub fn project_question(candidates: &[String]) -> Question {
    let mut seen = std::collections::BTreeSet::new();
    let mut criteria: Vec<Criterion> = candidates
        .iter()
        .filter(|p| !p.is_empty() && p.as_str() != reserved::CURRENT && seen.insert(p.as_str()))
        .take(MAX_PROJECT_CRITERIA)
        .map(|p| Criterion::new(p.clone(), ""))
        .collect();
    criteria.push(Criterion::new(
        reserved::CURRENT,
        "The current project; no other project is named.",
    ));
    Question::choice(
        "Which project does the user mean? Names may be spoken with spaces and \
         without accents.",
        criteria,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{MAX_CHOICE_CRITERIA, MIN_CHOICE_CRITERIA, Request};

    fn valid(q: Question) -> bool {
        Request {
            model: "m".into(),
            state: "{}".into(),
            questions: vec![("q".into(), q)],
            keep_alive: None,
        }
        .validate()
        .is_ok()
    }

    #[test]
    fn keys_round_trip() {
        for a in Action::ALL {
            assert_eq!(Action::from_key(a.key()), Some(*a));
        }
        for i in Intent::ALL {
            assert_eq!(Intent::from_key(i.key()), Some(*i));
        }
        for e in Engine::ALL {
            assert_eq!(Engine::from_key(e.key()), Some(*e));
        }
        assert_eq!(Action::from_key("launch"), None);
    }

    #[test]
    fn action_classes_follow_the_spec() {
        let rev: Vec<_> = Action::ALL.iter().filter(|a| a.is_reversible()).collect();
        assert_eq!(rev, [&Action::Pause, &Action::Status, &Action::Open]);
        let des: Vec<_> = Action::ALL.iter().filter(|a| a.is_destructive()).collect();
        assert_eq!(des, [&Action::Cancel, &Action::Deny]);
    }

    #[test]
    fn fixed_questions_are_valid_and_ordered() {
        assert!(valid(intent_question()));
        assert!(valid(action_question()));
        assert!(valid(engine_question()));
        for i in Intent::ALL {
            assert!(
                INTENT_CLASSES.iter().any(|(_, c, _)| c == i),
                "{i:?} has no class"
            );
        }
        assert_eq!(intent_question().criteria.len(), INTENT_CLASSES.len());
        assert_eq!(intent_of_class("stop_agent"), Some(Intent::Control));
        assert_eq!(intent_of_class("dictate"), Some(Intent::Dictation));
        assert_eq!(intent_of_class("control"), None);
        let keys: Vec<_> = action_question().keys().map(str::to_string).collect();
        assert_eq!(
            keys,
            [
                "pause", "resume", "cancel", "status", "approve", "deny", "retry", "open", "spawn"
            ]
        );
    }

    #[test]
    fn dictation_triggers() {
        let t = |s: &str| dictation_trigger(s);
        assert_eq!(
            t("dicta: añade un test para el parser").as_deref(),
            Some("añade un test para el parser")
        );
        assert_eq!(
            t("Dicta arregla el warning").as_deref(),
            Some("arregla el warning")
        );
        assert_eq!(
            t("escribe: revisa los imports").as_deref(),
            Some("revisa los imports")
        );
        assert_eq!(t("Escribe : hola").as_deref(), Some("hola"));
        assert_eq!(
            t("toma nota: hay que revisar").as_deref(),
            Some("hay que revisar")
        );
        assert_eq!(
            t("pon en el cuadro de texto: añade logs").as_deref(),
            Some("añade logs")
        );
        assert_eq!(
            t("dicta el siguiente texto: actualiza").as_deref(),
            Some("actualiza")
        );
        assert_eq!(t("dicta:").as_deref(), Some(""));
        // Not dictation.
        assert_eq!(t("escribe un plan para migrar la autenticación"), None);
        assert_eq!(t("cancela migración auth"), None);
        assert_eq!(t("dictamen del juez"), None);
        assert_eq!(t("¿qué dicta la norma?"), None);
    }

    #[test]
    fn target_triggers() {
        use TargetTrigger::{Back, Talk};
        for (text, want) in [
            ("háblale a auth", Some(Talk)),
            ("Háblale al de los tests", Some(Talk)),
            ("habla con el de docs", Some(Talk)),
            ("quiero hablar con migración auth", Some(Talk)),
            ("pásame con el agente de docs", Some(Talk)),
            ("vuelve", Some(Back)),
            ("Vuelve.", Some(Back)),
            ("vuelve a Pando", Some(Back)),
            ("volver", Some(Back)),
            ("ya está, vuelve a Pando", Some(Back)),
            ("Pando, vuelve por favor", Some(Back)),
            ("vuelve a la sesión principal", Some(Back)),
            // Not target switches.
            ("vuelve a lanzar migración auth", None),
            ("vuelve a intentarlo", None),
            ("habla con", None),
            ("háblale", None),
            ("¿de qué habla con el cliente?", None),
            ("cancela migración auth", None),
            ("pando", None),
        ] {
            assert_eq!(target_trigger(text), want, "{text:?}");
        }
    }

    #[test]
    fn agent_question_bounds_and_reserved_keys() {
        let agents: Vec<AgentInfo> = (0..40)
            .map(|i| AgentInfo {
                id: format!("a{i}"),
                name: format!("Agent {i}"),
                ..AgentInfo::default()
            })
            .chain([
                AgentInfo {
                    id: "all".into(),
                    ..AgentInfo::default()
                },
                AgentInfo {
                    id: "a1".into(),
                    ..AgentInfo::default()
                },
            ])
            .collect();
        let q = agent_question(&agents);
        assert_eq!(q.criteria.len(), MAX_CHOICE_CRITERIA);
        assert!(valid(q.clone()));
        let keys: Vec<_> = q.keys().collect();
        assert_eq!(keys[keys.len() - 2..], ["all", "none"]);
        // With no agents, still a valid 2-option question.
        let q = agent_question(&[]);
        assert_eq!(q.criteria.len(), MIN_CHOICE_CRITERIA);
        assert!(valid(q));
    }

    #[test]
    fn project_question_caps_at_twelve_plus_current() {
        let names: Vec<String> = (0..30).map(|i| format!("p{i}")).collect();
        let q = project_question(&names);
        assert_eq!(q.criteria.len(), MAX_PROJECT_CRITERIA + 1);
        assert_eq!(q.criteria.last().map(|c| c.key.as_str()), Some("current"));
        assert!(
            !valid(project_question(&[])),
            "a lone `current` is not a choice"
        );
        assert!(valid(project_question(&["bitacora".into()])));
    }
}
