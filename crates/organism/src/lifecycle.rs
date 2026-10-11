//! Cell lifecycle beyond the reference: stable ring slots with compaction, death animation,
//! birth wave, the Delegating overlay and the `+N` fold of the oldest finished cells.

use std::sync::Arc;

use crate::hash_seed;
use crate::rng::{Mulberry32, hash_unit};
use crate::sim::{CellSim, Life, Organism};
use crate::spring::CellSpring;
use crate::types::{CellId, CellInput, Engine, Inputs, Status};

/// Id of the synthetic aggregate cell that stands for the folded ones.
pub const FOLD_ID: &str = "__fold__";

/// Waves closer than this are considered the same event (birth right after Delegating).
const WAVE_GATE_SECONDS: f64 = 0.2;

impl Organism {
    /// Start the 1.5 s Delegating overlay (a new task appeared): adds a wave and a soft
    /// bulge, then the body returns to its previous state. Call it from the adapter.
    pub fn delegate(&mut self) {
        self.machine.delegate(self.t);
        self.emit_wave();
    }

    /// The state shown to the user: the base state or `Delegating` during the overlay.
    #[must_use]
    pub fn shown_state(&self) -> crate::BodyState {
        self.machine.shown()
    }

    /// Cells currently collapsed into the `+N` aggregate, oldest first.
    #[must_use]
    pub fn folded(&self) -> &[CellId] {
        &self.ring.folded
    }

    /// Number of ring slots the angles are spread over.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.ring.slots
    }

    /// Angle of ring slot `slot` for the current slot count.
    #[must_use]
    pub fn slot_angle(&self, slot: usize) -> f64 {
        -std::f64::consts::FRAC_PI_2
            + self.tau() * slot as f64 / self.ring.slots.max(1) as f64
            + self.params.ring.slot_offset
    }

    /// Non-parity reconciliation of the cell set with the app's list.
    pub(crate) fn reconcile(&mut self, inputs: &Inputs<'_>) {
        let t = self.t;
        let c0 = crate::params::W_REF / 2.0;
        let rc = self.ring_radius();
        let death = self.params.durations.cell_death;
        let vesicles = self.params.pseudopod.vesicles;
        let phase_span = self.extras.phase_span;
        let born_old = self.extras.born_old;

        self.fold_inputs(inputs.cells);
        let list = std::mem::take(&mut self.ring.scratch);

        // 1. Cells the app no longer lists start dying; listed dying cells revive.
        for c in &mut self.cells {
            let listed = list.iter().any(|a| a.id == c.input.id);
            match (listed, c.life) {
                (false, Life::Alive) => {
                    c.life = Life::Dying { since: t };
                    self.ring.last_change = t;
                }
                (true, Life::Dying { .. }) => {
                    c.life = Life::Alive;
                    self.ring.last_change = t;
                }
                _ => {}
            }
        }

        let mut fresh: Vec<usize> = Vec::new();
        // 2. Update listed cells, create the new ones in the lowest free slot.
        for input in &list {
            if let Some(c) = self.cells.iter_mut().find(|c| c.input.id == input.id) {
                c.input.clone_from(input);
                continue;
            }
            let slot = lowest_free_slot(&self.cells);
            let seed = hash_seed(input.id.as_str());
            let phase = hash_unit(input.id.as_str()) * phase_span;
            let mut frng = Mulberry32::new(seed);
            let flow = (0..vesicles).map(|_| frng.next_f64()).collect();
            let seq = self.ring.seq;
            self.ring.seq += 1;
            self.ring.last_change = t;
            // The angle is fixed up below once the slot count is known.
            self.cells.push(CellSim {
                input: input.clone(),
                spring: CellSpring::at(c0, c0),
                born: if input.is_new { t } else { born_old },
                phase,
                hover: 0.0,
                flow,
                ang: 0.0,
                slot,
                seq,
                life: Life::Alive,
                fade: 1.0,
            });
            if !input.is_new {
                fresh.push(self.cells.len() - 1);
            }
            if input.is_new && t - self.ring.last_wave > WAVE_GATE_SECONDS {
                self.emit_wave();
            }
        }
        self.ring.scratch = list;

        // 3. Drop cells whose death animation is over.
        self.cells
            .retain(|c| !matches!(c.life, Life::Dying { since } if t - since >= death));

        // 4. Slots: compaction after a quiet period, then the slot count and angles.
        self.compact_slots();
        let highest = self.cells.iter().map(|c| c.slot + 1).max().unwrap_or(0);
        self.ring.slots = self.params.ring.min_slots.max(highest);
        let slots = self.ring.slots;
        let tau = self.tau();
        let offset = self.params.ring.slot_offset;
        for c in &mut self.cells {
            c.ang = -std::f64::consts::FRAC_PI_2 + tau * c.slot as f64 / slots as f64 + offset;
        }
        // Cells that were already there when first seen start on their slot; newborns
        // (`is_new`) keep the centre as their spawn point.
        for i in fresh {
            let c = &mut self.cells[i];
            c.spring = CellSpring::at(c0 + c.ang.cos() * rc, c0 + c.ang.sin() * rc);
        }
    }

    /// Close the gaps left by dead cells once nothing happened for `compactAfterSeconds`;
    /// cells keep their creation order and the springs make every move smooth.
    fn compact_slots(&mut self) {
        if self.t - self.ring.last_change < self.params.ring.compact_after_seconds {
            return;
        }
        let mut alive: Vec<usize> = (0..self.cells.len())
            .filter(|&i| matches!(self.cells[i].life, Life::Alive))
            .collect();
        let gaps = {
            let mut slots: Vec<usize> = alive.iter().map(|&i| self.cells[i].slot).collect();
            slots.sort_unstable();
            slots.iter().enumerate().any(|(k, &s)| s != k)
        };
        if !gaps {
            return;
        }
        alive.sort_by_key(|&i| self.cells[i].seq);
        for (k, i) in alive.into_iter().enumerate() {
            self.cells[i].slot = k;
        }
        self.ring.last_change = self.t;
    }

    /// Apply the 12-cell cap: when more cells are listed, the oldest done/paused ones collapse
    /// into one `+N` cell. Waiting, error, working and pending cells are never collapsed.
    fn fold_inputs(&mut self, inputs: &[CellInput]) {
        let max = self.params.ring.max_cells;
        self.ring.scratch.clear();
        self.ring.folded.clear();
        let collapsible = |a: &CellInput| matches!(a.status, Status::Done | Status::Paused);
        if inputs.len() > max {
            let need = inputs.len() - max + 1;
            let k = need.min(inputs.iter().filter(|a| collapsible(a)).count());
            if k >= 2 {
                let label = self.fold_label(k);
                let mut taken = 0;
                let mut placed = false;
                for input in inputs {
                    if taken < k && collapsible(input) {
                        taken += 1;
                        self.ring.folded.push(input.id.clone());
                        if !placed {
                            placed = true;
                            let mut agg = CellInput::new(FOLD_ID, Engine::Cli, Status::Done);
                            agg.label = label.clone();
                            self.ring.scratch.push(agg);
                        }
                    } else {
                        self.ring.scratch.push(input.clone());
                    }
                }
                return;
            }
        }
        self.ring.scratch.extend(inputs.iter().cloned());
    }

    fn fold_label(&mut self, k: usize) -> Arc<str> {
        match &self.ring.fold_label {
            Some((n, label)) if *n == k => label.clone(),
            _ => {
                let label: Arc<str> = Arc::from(format!("+{k}"));
                self.ring.fold_label = Some((k, label.clone()));
                label
            }
        }
    }
}

fn lowest_free_slot(cells: &[CellSim]) -> usize {
    let mut used = vec![false; cells.len() + 1];
    for c in cells {
        if let Some(u) = used.get_mut(c.slot) {
            *u = true;
        }
    }
    used.iter().position(|u| !u).unwrap_or(cells.len())
}
