//! Who the user's next message goes to (PDA-T-0062, `docs/06-ui-mapping.md` "Voice target"):
//! the primary session (mesnada) by default, or one cell the user chose with "Hablarle" until
//! "vuelve" / Esc. Pure state; the header pill renders it and the voice / chat layers read it.

use organism::{CellId, CellInput, Status};

/// Where prompts go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// The primary session.
    #[default]
    Primary,
    /// A cell that can talk, through its own session.
    Cell {
        /// Cell id.
        id: CellId,
        /// The task's chat/ACP session (gap G2); prompts cannot be routed without it.
        session_id: Option<String>,
        /// Label for the header pill.
        label: String,
    },
}

/// Why "Hablarle" was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TalkRefused {
    /// The engine only produces output (CLI).
    CannotTalk,
    /// The cell is finished.
    Finished,
}

/// The target and the rules that move it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetModel {
    target: Target,
}

impl TargetModel {
    /// Starts on the primary session.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current target.
    #[must_use]
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// The targeted cell, if any.
    #[must_use]
    pub fn cell(&self) -> Option<&CellId> {
        match &self.target {
            Target::Primary => None,
            Target::Cell { id, .. } => Some(id),
        }
    }

    /// Session prompts must go to, `None` = the primary session.
    #[must_use]
    pub fn session(&self) -> Option<&str> {
        match &self.target {
            Target::Cell { session_id, .. } => session_id.as_deref(),
            Target::Primary => None,
        }
    }

    /// "Hablarle": talk to `cell` from now on.
    pub fn talk_to(
        &mut self,
        cell: &CellInput,
        session_id: Option<String>,
    ) -> Result<(), TalkRefused> {
        if !cell.engine.token().can_talk {
            return Err(TalkRefused::CannotTalk);
        }
        if matches!(cell.status, Status::Done | Status::Error) {
            return Err(TalkRefused::Finished);
        }
        self.target = Target::Cell {
            id: cell.id.clone(),
            session_id,
            label: cell.label.to_string(),
        };
        Ok(())
    }

    /// Esc / "vuelve": back to the primary session. Returns whether something changed.
    pub fn back(&mut self) -> bool {
        std::mem::take(&mut self.target) != Target::Primary
    }

    /// Cells changed: a targeted cell that is gone or finished returns to the primary.
    pub fn on_cells(&mut self, cells: &[CellInput]) -> bool {
        let Target::Cell { id, .. } = &self.target else {
            return false;
        };
        let alive = cells
            .iter()
            .any(|c| &c.id == id && !matches!(c.status, Status::Done | Status::Error));
        if alive {
            return false;
        }
        self.target = Target::Primary;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use organism::Engine;

    fn c(id: &str, engine: Engine, status: Status) -> CellInput {
        CellInput::new(id, engine, status)
    }

    #[test]
    fn starts_on_the_primary_and_hablarle_moves_to_a_cell() {
        let mut m = TargetModel::new();
        assert_eq!(m.target(), &Target::Primary);
        assert_eq!(m.session(), None);
        m.talk_to(
            &c("a", Engine::ClaudeAcp, Status::Working),
            Some("s1".into()),
        )
        .expect("acp can talk");
        assert_eq!(m.cell().map(CellId::as_str), Some("a"));
        assert_eq!(m.session(), Some("s1"));
    }

    #[test]
    fn cli_and_finished_cells_refuse() {
        let mut m = TargetModel::new();
        assert_eq!(
            m.talk_to(&c("a", Engine::Cli, Status::Working), None),
            Err(TalkRefused::CannotTalk)
        );
        assert_eq!(
            m.talk_to(&c("a", Engine::Pando, Status::Done), None),
            Err(TalkRefused::Finished)
        );
        assert_eq!(m.target(), &Target::Primary);
    }

    #[test]
    fn escape_returns_to_the_primary() {
        let mut m = TargetModel::new();
        assert!(!m.back());
        m.talk_to(&c("a", Engine::Pando, Status::Waiting), None)
            .expect("ok");
        assert!(m.back());
        assert_eq!(m.target(), &Target::Primary);
    }

    #[test]
    fn a_vanished_or_finished_target_falls_back() {
        let mut m = TargetModel::new();
        m.talk_to(&c("a", Engine::Pando, Status::Working), None)
            .expect("ok");
        assert!(!m.on_cells(&[c("a", Engine::Pando, Status::Waiting)]));
        assert!(m.on_cells(&[c("a", Engine::Pando, Status::Done)]));
        m.talk_to(&c("a", Engine::Pando, Status::Working), None)
            .expect("ok");
        assert!(m.on_cells(&[]));
        assert_eq!(m.target(), &Target::Primary);
    }
}
