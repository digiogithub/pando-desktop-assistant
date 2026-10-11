//! Public input types of the organism.

use std::sync::Arc;

use crate::tokens;

/// What the voice pipeline is doing (the state of the body).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BodyState {
    #[default]
    Idle,
    Listening,
    Thinking,
    Speaking,
    Delegating,
}

impl BodyState {
    pub const ALL: [BodyState; 5] = [
        BodyState::Idle,
        BodyState::Listening,
        BodyState::Thinking,
        BodyState::Speaking,
        BodyState::Delegating,
    ];
}

/// Who runs a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Engine {
    Pando,
    ClaudeAcp,
    ClaudeExternal,
    Cli,
}

impl Engine {
    /// The design-token row for this engine.
    #[must_use]
    pub fn token(self) -> &'static tokens::EngineToken {
        &tokens::ENGINES[self as usize]
    }
}

/// How a task is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// Queued: 60 % alpha, nucleus still, vesicles flow outward.
    Pending,
    Working,
    Waiting,
    Paused,
    Done,
    Error,
}

impl Status {
    /// Paused, done and error cells are "dead" for pseudopods and the bulge.
    #[must_use]
    pub fn is_dead(self) -> bool {
        matches!(self, Status::Paused | Status::Done | Status::Error)
    }

    /// Token id (`pending`, `working`, ...).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Working => "working",
            Status::Waiting => "waiting",
            Status::Paused => "paused",
            Status::Done => "done",
            Status::Error => "error",
        }
    }
}

/// Stable identity of a cell (an agent / task id).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellId(pub Arc<str>);

impl CellId {
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self(Arc::from(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CellId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// A cell as the app describes it each frame.
#[derive(Clone, Debug, PartialEq)]
pub struct CellInput {
    pub id: CellId,
    pub engine: Engine,
    pub status: Status,
    /// Spawn at the body centre with the birth animation (mitosis) instead of at the slot.
    pub is_new: bool,
    /// Extra alpha multiplier in `0..=1` (1 = none).
    pub dim: f32,
    /// Label drawn under the cell.
    pub label: Arc<str>,
}

impl CellInput {
    #[must_use]
    pub fn new(id: &str, engine: Engine, status: Status) -> Self {
        Self {
            id: CellId::new(id),
            engine,
            status,
            is_new: false,
            dim: 1.0,
            label: Arc::from(id),
        }
    }

    #[must_use]
    pub fn with_new(mut self, is_new: bool) -> Self {
        self.is_new = is_new;
        self
    }
}

/// Everything the simulation reads on a frame.
#[derive(Clone, Debug)]
pub struct Inputs<'a> {
    pub state: BodyState,
    /// Smoothed mic RMS in `0..=1`; `None` uses the synthetic envelope.
    pub mic_rms: Option<f32>,
    /// Smoothed TTS RMS in `0..=1`; `None` uses the synthetic envelope.
    pub tts_rms: Option<f32>,
    /// Pointer in normalised canvas space (`0..=1`), `None` when it left the canvas.
    pub pointer: Option<[f32; 2]>,
    pub selected: Option<CellId>,
    pub cells: &'a [CellInput],
    pub reduced_motion: bool,
    /// Canvas width in logical px (hover radius floor); 640 = reference.
    pub canvas_px: f32,
}

impl<'a> Inputs<'a> {
    #[must_use]
    pub fn new(state: BodyState, cells: &'a [CellInput]) -> Self {
        Self {
            state,
            mic_rms: None,
            tts_rms: None,
            pointer: None,
            selected: None,
            cells,
            reduced_motion: false,
            canvas_px: 640.0,
        }
    }
}
