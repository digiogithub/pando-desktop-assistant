//! Pure helpers shared by every cell source (demo, Pando tasks, instances).

use std::collections::HashMap;

use organism::{CellId, CellInput, Status};

/// Seconds a finished cell stays on the organism before the adapter removes it
/// (`durations.removeTerminalAfter`); the organism animates its death.
pub const REMOVE_TERMINAL_AFTER: f64 = 10.0;

/// Removes `Done` / `Error` cells `REMOVE_TERMINAL_AFTER` seconds after they became terminal.
#[derive(Clone, Debug, Default)]
pub struct TerminalReaper {
    since: HashMap<CellId, f64>,
    /// Seconds after which a terminal cell is dropped.
    pub after: f64,
}

impl TerminalReaper {
    #[must_use]
    pub fn new() -> Self {
        Self {
            since: HashMap::new(),
            after: REMOVE_TERMINAL_AFTER,
        }
    }

    /// Statuses that end a cell's life on the ring.
    #[must_use]
    pub fn is_terminal(status: Status) -> bool {
        matches!(status, Status::Done | Status::Error)
    }

    /// Updates the clocks with `now` (seconds) and drops expired terminal cells; returns the
    /// removed ids.
    pub fn reap(&mut self, now: f64, cells: &mut Vec<CellInput>) -> Vec<CellId> {
        let live: Vec<&CellId> = cells.iter().map(|c| &c.id).collect();
        self.since.retain(|id, _| live.contains(&id));
        for c in cells.iter() {
            if Self::is_terminal(c.status) {
                self.since.entry(c.id.clone()).or_insert(now);
            } else {
                self.since.remove(&c.id);
            }
        }
        let expired: Vec<CellId> = self
            .since
            .iter()
            .filter(|(_, since)| now - **since >= self.after)
            .map(|(id, _)| id.clone())
            .collect();
        cells.retain(|c| !expired.contains(&c.id));
        for id in &expired {
            self.since.remove(id);
        }
        expired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use organism::Engine;

    #[test]
    fn terminal_cells_expire_after_ten_seconds() {
        let mut r = TerminalReaper::new();
        let mut cells = vec![
            CellInput::new("a", Engine::Pando, Status::Working),
            CellInput::new("b", Engine::Pando, Status::Done),
        ];
        assert!(r.reap(0.0, &mut cells).is_empty());
        assert!(r.reap(9.9, &mut cells).is_empty());
        let gone = r.reap(10.0, &mut cells);
        assert_eq!(gone, vec![CellId::new("b")]);
        assert_eq!(cells.len(), 1);
    }

    #[test]
    fn a_revived_cell_resets_its_clock() {
        let mut r = TerminalReaper::new();
        let mut cells = vec![CellInput::new("a", Engine::Cli, Status::Error)];
        r.reap(0.0, &mut cells);
        cells[0].status = Status::Working;
        r.reap(5.0, &mut cells);
        cells[0].status = Status::Done;
        r.reap(6.0, &mut cells);
        assert!(r.reap(15.0, &mut cells).is_empty());
        assert_eq!(r.reap(16.0, &mut cells).len(), 1);
    }
}
