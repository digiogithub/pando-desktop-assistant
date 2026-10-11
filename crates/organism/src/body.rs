//! The body membrane: [`Organism::body_radius`] with the per-cell bulge terms cached once per
//! step (the radius is evaluated 160 + 70 + n times per frame).

use crate::noise::noise;
use crate::params::W_REF;
use crate::sim::Organism;
use crate::types::BodyState;

/// Per-step cache of everything in `bodyR` that does not depend on the angle.
#[derive(Clone, Debug, Default)]
pub(crate) struct BodyCache {
    pub base: f64,
    noise_amp: f64,
    breath_term: f64,
    ripple_gain: f64,
    /// `(angle of the cell, base * (amount * alive + birth))`.
    bulges: Vec<(f64, f64)>,
    two_sigma_sq: f64,
}

impl Organism {
    /// Body radius multiplier of the *visual* state.
    pub(crate) fn body_state_factor(&self, state: BodyState) -> f64 {
        let by = self.params.body_radius_by_state;
        match state {
            BodyState::Idle => by.idle,
            BodyState::Listening => by.listening,
            BodyState::Thinking => by.thinking,
            BodyState::Speaking => by.speaking,
            BodyState::Delegating => by.delegating,
        }
    }

    /// Recompute the cache; called at the end of every `step`.
    pub(crate) fn refresh_body(&mut self) {
        let p = &self.params;
        let rm = self.reduced_motion;
        let base = W_REF * p.body_radius * self.body_state_factor(self.visual_state());
        let b = p.bulge;
        let mut bulges = std::mem::take(&mut self.body.bulges);
        bulges.clear();
        for c in &self.cells {
            let theta = (c.spring.y - W_REF / 2.0).atan2(c.spring.x - W_REF / 2.0);
            let age = self.t - c.born;
            let birth = if age < b.birth_seconds {
                (1.0 - age / b.birth_seconds) * b.birth_extra
            } else {
                0.0
            };
            let alive = if c.input.status.is_dead() {
                b.dead_factor
            } else {
                1.0
            };
            let fade = self.bulge_fade(c);
            bulges.push((theta, base * (b.amount * alive + birth) * fade));
        }
        self.body = BodyCache {
            base,
            noise_amp: if rm { 0.0 } else { p.membrane.amplitude },
            breath_term: if rm {
                0.0
            } else {
                p.membrane.breath * (self.t * p.membrane.breath_rad_per_second).sin()
            },
            ripple_gain: if rm {
                0.0
            } else {
                base * self.amp.value * p.voice_ripple.amplitude
            },
            bulges,
            two_sigma_sq: 2.0 * b.sigma * b.sigma,
        };
    }

    /// Body membrane radius at `angle` (reference units).
    #[must_use]
    pub fn body_radius(&self, angle: f64) -> f64 {
        let c = &self.body;
        let p = &self.params;
        let t = self.t;
        let mut r =
            c.base * (1.0 + c.noise_amp * noise(&p.membrane, angle, t, 0.0) + c.breath_term);
        if c.ripple_gain != 0.0 {
            let v = p.voice_ripple;
            r += c.ripple_gain
                * (v.angular[0] * angle + t * v.temporal[0]).sin()
                * (v.angular[1] * angle + t * v.temporal[1]).sin();
        }
        for &(theta, coef) in &c.bulges {
            let d0 = theta - angle;
            let d = d0.sin().atan2(d0.cos());
            r += coef * (-(d * d) / c.two_sigma_sq).exp();
        }
        r + self.overlay_extra(c.base)
    }

    /// Radius of a cell including birth growth and hover (reference units).
    #[must_use]
    pub fn cell_radius(&self, c: &crate::sim::CellSim) -> f64 {
        let age = self.t - c.born;
        let g = if age < self.params.bulge.birth_seconds {
            (age / self.extras.cell_growth_seconds).min(1.0)
        } else {
            1.0
        };
        W_REF * self.params.cell.radius * g * (1.0 + (self.params.cell.hover_scale - 1.0) * c.hover)
    }
}
