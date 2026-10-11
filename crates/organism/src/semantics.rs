//! Accessible description of the cells: for GPUI hitboxes, tooltips, keyboard focus and the
//! accessibility tree. Everything is in the logical pixels of the [`Scene`].

use std::sync::Arc;

use crate::layout::Scene;
use crate::sim::{Life, Organism};
use crate::types::{CellId, Engine, Status};

/// Axis-aligned rectangle in logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[must_use]
    pub fn centre(&self) -> [f32; 2] {
        [self.x + self.w / 2.0, self.y + self.h / 2.0]
    }

    #[must_use]
    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[0] <= self.x + self.w && p[1] >= self.y && p[1] <= self.y + self.h
    }
}

/// One cell as the accessibility tree and tooltips need it.
#[derive(Clone, Debug, PartialEq)]
pub struct CellSemantics {
    pub id: CellId,
    /// Display label supplied by the app (already localized by the caller).
    pub label: Arc<str>,
    pub engine: Engine,
    /// Stable ids to look up localized strings (`organism.engine.<id>`, `organism.status.<id>`).
    pub engine_id: &'static str,
    pub status: Status,
    pub status_id: &'static str,
    /// English fallbacks from the design tokens.
    pub engine_label: &'static str,
    /// Bounds of the drawn cell (membrane included), centred on the cell.
    pub bounds: Rect,
    /// Pointer target radius: `max(click radius * scale, min hit radius)`.
    pub hit_radius: f32,
    pub selected: bool,
    pub hovered: bool,
    /// True for the `+N` aggregate; `folded` lists the cells it stands for.
    pub aggregate: bool,
    pub folded: Vec<CellId>,
}

impl Organism {
    /// Cells for the accessibility tree, in ring order (slot order), skipping dying cells.
    #[must_use]
    pub fn semantics(&self, scene: &Scene) -> Vec<CellSemantics> {
        let mut out = Vec::with_capacity(self.cells.len());
        self.semantics_into(scene, &mut out);
        out
    }

    /// Like [`semantics`](Self::semantics), reusing the buffer.
    pub fn semantics_into(&self, scene: &Scene, out: &mut Vec<CellSemantics>) {
        out.clear();
        let s = f64::from(scene.scale);
        let noise_pad = 1.0
            + self
                .params
                .cell
                .noise_working
                .max(self.params.cell.noise_other)
                * self
                    .params
                    .membrane
                    .harmonics
                    .iter()
                    .map(|h| h[2].abs())
                    .sum::<f64>();
        let hit = (self.extras.click_radius * s).max(self.params.scaling.min_hit_radius_px) as f32;
        for c in &self.cells {
            if !matches!(c.life, Life::Alive) {
                continue;
            }
            let r = (self.cell_radius(c) * noise_pad * s) as f32;
            let (cx, cy) = ((c.spring.x * s) as f32, (c.spring.y * s) as f32);
            let aggregate = c.input.id.as_str() == crate::lifecycle::FOLD_ID;
            let token = c.input.engine.token();
            out.push(CellSemantics {
                id: c.input.id.clone(),
                label: c.input.label.clone(),
                engine: c.input.engine,
                engine_id: token.id,
                status: c.input.status,
                status_id: c.input.status.id(),
                engine_label: token.label,
                bounds: Rect {
                    x: cx - r,
                    y: cy - r,
                    w: 2.0 * r,
                    h: 2.0 * r,
                },
                hit_radius: hit,
                selected: self.selected.as_ref() == Some(&c.input.id),
                hovered: c.hover > 0.5,
                aggregate,
                folded: if aggregate {
                    self.ring.folded.clone()
                } else {
                    Vec::new()
                },
            });
        }
        out.sort_by_key(|c| {
            self.cells
                .iter()
                .find(|x| x.input.id == c.id)
                .map_or(usize::MAX, |x| x.slot)
        });
    }

    /// The cell under `point` (logical px): the nearest one within its hit radius.
    #[must_use]
    pub fn hit_test(&self, scene: &Scene, point: [f32; 2]) -> Option<CellId> {
        let s = f64::from(scene.scale);
        let hit = (self.extras.click_radius * s).max(self.params.scaling.min_hit_radius_px);
        let mut best: Option<(f64, &CellId)> = None;
        for c in self.cells.iter().filter(|c| matches!(c.life, Life::Alive)) {
            let d =
                (c.spring.x * s - f64::from(point[0])).hypot(c.spring.y * s - f64::from(point[1]));
            if d < hit && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, &c.input.id));
            }
        }
        best.map(|(_, id)| id.clone())
    }
}
