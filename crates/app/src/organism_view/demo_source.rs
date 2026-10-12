//! Scripted fake agents for the demo (`--demo`) and for visual checks. Pure and seeded.

use organism::{CellId, CellInput, Engine, Status};

use super::adapter::TerminalReaper;

/// Most cells the script keeps alive (more than the 12-cell cap shows the `+N` fold).
const MAX_CELLS: usize = 15;

/// Tiny xorshift generator (the demo needs variety, not quality).
#[derive(Clone, Debug)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn range(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

const NAMES: [&str; 12] = [
    "auth", "docs", "build", "tests", "deploy", "lint", "api", "ui", "db", "perf", "i18n", "audio",
];

/// What the script did on a tick.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Cells were added, removed or changed status.
    pub changed: bool,
    /// A cell was added (the adapter should call `Organism::delegate`).
    pub spawned: bool,
}

/// A deterministic stream of spawn / status / removal events.
#[derive(Clone, Debug)]
pub struct DemoSource {
    rng: Rng,
    t: f64,
    next_event: f64,
    seq: u32,
    cells: Vec<CellInput>,
    reaper: TerminalReaper,
}

impl DemoSource {
    /// Starts with three agents in different states.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut s = Self {
            rng: Rng(seed.max(1)),
            t: 0.0,
            next_event: 2.5,
            seq: 0,
            cells: Vec::new(),
            reaper: TerminalReaper::new(),
        };
        s.push(Engine::Pando, Status::Working, false);
        s.push(Engine::ClaudeAcp, Status::Waiting, false);
        s.push(Engine::Cli, Status::Pending, false);
        s
    }

    fn push(&mut self, engine: Engine, status: Status, is_new: bool) {
        self.seq += 1;
        let name = NAMES[(self.seq as usize - 1) % NAMES.len()];
        let id = format!("{name}-{}", self.seq);
        let mut c = CellInput::new(&id, engine, status).with_new(is_new);
        c.label = id.as_str().into();
        self.cells.push(c);
    }

    /// Current cells.
    #[must_use]
    pub fn cells(&self) -> &[CellInput] {
        &self.cells
    }

    /// Adds an agent with a random engine (pending, with the birth animation).
    pub fn spawn(&mut self) -> bool {
        if self.cells.len() >= MAX_CELLS {
            return false;
        }
        let engine = [
            Engine::Pando,
            Engine::Pando,
            Engine::ClaudeAcp,
            Engine::ClaudeExternal,
            Engine::Cli,
        ][self.rng.range(5)];
        self.push(engine, Status::Pending, true);
        true
    }

    /// Removes the oldest cell.
    pub fn remove_oldest(&mut self) -> Option<CellId> {
        if self.cells.is_empty() {
            None
        } else {
            Some(self.cells.remove(0).id)
        }
    }

    /// Moves a cell to its next status in the demo cycle.
    pub fn cycle_status(&mut self, id: &CellId) -> bool {
        let Some(c) = self.cells.iter_mut().find(|c| &c.id == id) else {
            return false;
        };
        c.status = next_status(c.status);
        c.is_new = false;
        true
    }

    /// Advances the script by `dt` seconds.
    pub fn tick(&mut self, dt: f64) -> Changes {
        self.t += dt;
        let mut out = Changes::default();
        for c in &mut self.cells {
            c.is_new = false;
        }
        if self.t >= self.next_event {
            self.next_event = self.t + 2.0 + self.rng.unit() * 4.0;
            let roll = self.rng.range(10);
            if roll < 3 {
                out.spawned = self.spawn();
                out.changed |= out.spawned;
            } else if !self.cells.is_empty() {
                let i = self.rng.range(self.cells.len());
                let c = &mut self.cells[i];
                if !TerminalReaper::is_terminal(c.status) {
                    c.status = next_status(c.status);
                    out.changed = true;
                }
            }
        }
        if !self.reaper.reap(self.t, &mut self.cells).is_empty() {
            out.changed = true;
        }
        out
    }
}

/// pending -> working -> waiting -> working -> done / error / paused ... (terminal stays).
fn next_status(s: Status) -> Status {
    match s {
        Status::Pending => Status::Working,
        Status::Working => Status::Waiting,
        Status::Waiting => Status::Done,
        Status::Paused => Status::Working,
        Status::Done => Status::Error,
        Status::Error => Status::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_is_deterministic_and_bounded() {
        let run = |seed| {
            let mut s = DemoSource::new(seed);
            for _ in 0..3000 {
                s.tick(0.1);
                assert!(s.cells().len() <= MAX_CELLS);
            }
            s.cells().iter().map(|c| c.id.clone()).collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
    }

    #[test]
    fn something_spawns_and_finished_cells_leave() {
        let mut s = DemoSource::new(3);
        let mut spawned = 0;
        let mut removed = false;
        let mut max = 0;
        for _ in 0..6000 {
            let before = s.cells().len();
            let ch = s.tick(0.1);
            spawned += usize::from(ch.spawned);
            removed |= s.cells().len() < before;
            max = max.max(s.cells().len());
        }
        assert!(spawned > 3 && removed && max >= 3);
    }

    #[test]
    fn manual_controls() {
        let mut s = DemoSource::new(1);
        let n = s.cells().len();
        assert!(s.spawn());
        assert_eq!(s.cells().len(), n + 1);
        let id = s.cells()[0].id.clone();
        assert!(s.cycle_status(&id));
        assert_eq!(s.remove_oldest(), Some(id));
    }
}
