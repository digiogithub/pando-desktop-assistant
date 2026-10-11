//! The simulation: [`Organism::step`] advances amplitude, waves, cells, organelles and the
//! nucleus. Pure and deterministic; all lengths are in the 640-unit reference space.
//!
//! `Config::parity` reproduces `design-system/reference/organism.html` exactly (used by the
//! golden tests); without it the lifecycle improvements of the design decisions apply.

use std::f64::consts::TAU;

use crate::envelope::Amplitude;
use crate::params::{Extras, Params, W_REF};
use crate::rng::{Mulberry32, hash_unit};
use crate::spring::CellSpring;
use crate::types::{BodyState, CellId, CellInput, Inputs, Status};

/// Largest `dt` the simulation accepts (the reference clamps the frame time to 50 ms).
pub const MAX_DT: f64 = 0.05;

/// Simulation configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Reproduce the reference exactly (RNG phases, slot reflow, instant deletion, `6.283`).
    pub parity: bool,
    /// Seed of the organelle / wave RNG.
    pub seed: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            parity: false,
            seed: 1,
        }
    }
}

/// A wave ring expanding from the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wave {
    pub r: f64,
    pub life: f64,
}

/// A particle inside the body (polar: angle, radial fraction).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Organelle {
    pub a: f64,
    pub r: f64,
    pub s: f64,
    pub sz: f64,
}

/// One simulated cell.
#[derive(Clone, Debug)]
pub struct CellSim {
    pub input: CellInput,
    pub spring: CellSpring,
    pub born: f64,
    pub phase: f64,
    pub hover: f64,
    pub flow: Vec<f64>,
    /// Angle of the ring slot the cell is pulled to.
    pub ang: f64,
}

impl CellSim {
    #[must_use]
    pub fn id(&self) -> &CellId {
        &self.input.id
    }
}

/// The organism simulation state.
#[derive(Clone, Debug)]
pub struct Organism {
    pub(crate) params: Params,
    pub(crate) extras: Extras,
    pub(crate) cfg: Config,
    pub(crate) t: f64,
    pub(crate) amp: Amplitude,
    pub(crate) nucleus: [f64; 2],
    pub(crate) cells: Vec<CellSim>,
    pub(crate) organelles: Vec<Organelle>,
    pub(crate) waves: Vec<Wave>,
    pub(crate) rng: Mulberry32,
    pub(crate) last_state: BodyState,
    pub(crate) state: BodyState,
    pub(crate) selected: Option<CellId>,
    pub(crate) reduced_motion: bool,
    pub(crate) pointer: Option<[f64; 2]>,
}

impl Organism {
    /// Create an organism with the default tokens and extras.
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        Self::with_params(Params::default(), Extras::default(), cfg)
    }

    #[must_use]
    pub fn with_params(params: Params, extras: Extras, cfg: Config) -> Self {
        let mut rng = Mulberry32::new(cfg.seed);
        let tau = if cfg.parity {
            extras.tau_reference
        } else {
            TAU
        };
        let o = params.organelles;
        let organelles = (0..o.count)
            .map(|_| {
                // Evaluation order matches the reference object literal: a, r, s, sz.
                let a = rng.next_f64() * tau;
                let r = rng.next_f64();
                let s = extras.organelle_speed_min + rng.next_f64() * extras.organelle_speed_span;
                let sz = o.size_min + rng.next_f64() * (o.size_max - o.size_min);
                Organelle { a, r, s, sz }
            })
            .collect();
        Self {
            params,
            extras,
            cfg,
            t: 0.0,
            amp: Amplitude::default(),
            nucleus: [W_REF / 2.0, W_REF / 2.0],
            cells: Vec::new(),
            organelles,
            waves: Vec::new(),
            rng,
            last_state: BodyState::Idle,
            state: BodyState::Idle,
            selected: None,
            reduced_motion: false,
            pointer: None,
        }
    }

    // --- read access -------------------------------------------------------------------

    #[must_use]
    pub fn time(&self) -> f64 {
        self.t
    }

    #[must_use]
    pub fn amplitude(&self) -> f64 {
        self.amp.value
    }

    #[must_use]
    pub fn amplitude_clock(&self) -> f64 {
        self.amp.tau
    }

    #[must_use]
    pub fn nucleus(&self) -> [f64; 2] {
        self.nucleus
    }

    #[must_use]
    pub fn cells(&self) -> &[CellSim] {
        &self.cells
    }

    #[must_use]
    pub fn organelles(&self) -> &[Organelle] {
        &self.organelles
    }

    #[must_use]
    pub fn waves(&self) -> &[Wave] {
        &self.waves
    }

    #[must_use]
    pub fn state(&self) -> BodyState {
        self.state
    }

    #[must_use]
    pub fn params(&self) -> &Params {
        &self.params
    }

    #[must_use]
    pub fn extras(&self) -> &Extras {
        &self.extras
    }

    #[must_use]
    pub fn config(&self) -> Config {
        self.cfg
    }

    pub(crate) fn tau(&self) -> f64 {
        if self.cfg.parity {
            self.extras.tau_reference
        } else {
            TAU
        }
    }

    /// Radius of the ring cells orbit on, in reference units.
    #[must_use]
    pub(crate) fn ring_radius(&self) -> f64 {
        W_REF * self.params.ring_radius
    }

    // --- step --------------------------------------------------------------------------

    /// Advance the simulation by `dt` seconds (clamped to [`MAX_DT`]).
    ///
    /// Order mirrors the reference `step()` and the nucleus tracking that `draw()` did:
    /// amplitude, state/wave emission, wave advance, cell reconciliation + spring/flow/hover,
    /// organelles, nucleus.
    pub fn step(&mut self, dt: f64, inputs: &Inputs<'_>) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, MAX_DT)
        } else {
            0.0
        };
        self.t += dt;
        self.reduced_motion = inputs.reduced_motion && !self.cfg.parity;
        self.pointer = inputs
            .pointer
            .map(|p| [p[0] * W_REF, p[1] * W_REF])
            .filter(|p| p[0].is_finite() && p[1].is_finite());
        self.selected.clone_from(&inputs.selected);
        let state = inputs.state;
        self.state = state;

        self.step_amplitude(dt, inputs);
        self.step_waves(dt, state);
        self.step_cells(dt, inputs);
        self.step_organelles(dt, state);
        self.step_nucleus(dt);
    }

    fn step_amplitude(&mut self, dt: f64, inputs: &Inputs<'_>) {
        let rms = match inputs.state {
            BodyState::Listening => inputs.mic_rms,
            BodyState::Speaking => inputs.tts_rms,
            _ => None,
        };
        self.amp
            .update(&self.params, inputs.state, rms.map(f64::from), dt);
    }

    fn emit_wave(&mut self) {
        self.waves.push(Wave { r: 0.0, life: 1.0 });
    }

    fn step_waves(&mut self, dt: f64, state: BodyState) {
        if state != self.last_state {
            if matches!(state, BodyState::Speaking | BodyState::Delegating) {
                self.emit_wave();
            }
            self.last_state = state;
        }
        if state == BodyState::Speaking
            && self.rng.next_f64() < dt * self.params.waves.speaking_rate_per_second
        {
            self.emit_wave();
        }
        let speed = W_REF * self.params.waves.speed;
        let decay = self.params.waves.decay_per_second;
        let rm = self.reduced_motion;
        for w in &mut self.waves {
            // Reduced motion: the ring fades in place instead of expanding.
            if !rm {
                w.r += dt * speed;
            }
            w.life -= dt * decay;
        }
        self.waves.retain(|w| w.life > 0.0);
    }

    fn step_cells(&mut self, dt: f64, inputs: &Inputs<'_>) {
        let c0 = W_REF / 2.0;
        let rc = self.ring_radius();
        let tau = self.tau();
        let n = inputs.cells.len();
        let birth_seconds = self.params.bulge.birth_seconds;
        let ph = self.params.physics;
        let ps = self.params.pseudopod;
        let ex = self.extras.clone();
        let hover_r = hover_radius(&ex, &self.params, inputs.canvas_px);

        for (i, input) in inputs.cells.iter().enumerate() {
            let ang = -std::f64::consts::FRAC_PI_2 + (tau / n as f64) * i as f64 + 0.15;
            let idx = match self.cells.iter().position(|c| c.input.id == input.id) {
                Some(idx) => idx,
                None => {
                    let born = input.is_new;
                    let (x, y) = if born {
                        (c0, c0)
                    } else {
                        (c0 + ang.cos() * rc, c0 + ang.sin() * rc)
                    };
                    let phase = self.rng.next_f64() * ex.phase_span;
                    let flow = (0..ps.vesicles).map(|_| self.rng.next_f64()).collect();
                    self.cells.push(CellSim {
                        input: input.clone(),
                        spring: CellSpring::at(x, y),
                        born: if born { self.t } else { ex.born_old },
                        phase,
                        hover: 0.0,
                        flow,
                        ang,
                    });
                    self.cells.len() - 1
                }
            };
            let t = self.t;
            let thinking = self.state == BodyState::Thinking;
            let rm = self.reduced_motion;
            let pointer = self.pointer;
            let c = &mut self.cells[idx];
            c.ang = ang;
            c.input.clone_from(input);
            let age = t - c.born;
            let drift = if rm {
                0.0
            } else {
                ex.drift_amplitude * (t * ex.drift_rate + c.phase).sin()
            };
            let tx = c0 + ang.cos() * (rc + drift);
            let ty = c0 + ang.sin() * (rc + drift * ex.drift_y_ratio);
            if rm {
                c.spring.step_critical(tx, ty, 6.0, dt);
            } else {
                let k = if age < birth_seconds {
                    ph.cell_spring_k_newborn
                } else {
                    ph.cell_spring_k
                };
                c.spring.step(
                    tx,
                    ty,
                    k,
                    ph.cell_damping_per_second,
                    ph.cell_position_gain,
                    dt,
                );
            }
            let base = match input.status {
                Status::Working | Status::Pending => ps.flow_working,
                Status::Waiting => ps.flow_waiting,
                _ => 0.0,
            };
            let spd = if rm {
                0.0
            } else {
                base * if thinking {
                    ps.flow_thinking_multiplier
                } else {
                    1.0
                }
            };
            for f in &mut c.flow {
                *f = ((*f + spd * dt) % 1.0 + 1.0) % 1.0;
            }
            let hov =
                pointer.is_some_and(|m| (m[0] - c.spring.x).hypot(m[1] - c.spring.y) < hover_r);
            c.hover +=
                (f64::from(u8::from(hov)) - c.hover) * (dt * ph.hover_lerp_per_second).min(1.0);
        }

        // Cells whose agent disappeared are removed instantly (reference behaviour).
        let inputs_cells = inputs.cells;
        self.cells
            .retain(|c| inputs_cells.iter().any(|a| a.id == c.input.id));
    }

    fn step_organelles(&mut self, dt: f64, state: BodyState) {
        if self.reduced_motion {
            return;
        }
        let o = self.params.organelles;
        let sp = match state {
            BodyState::Idle => o.speed_idle,
            BodyState::Thinking => o.speed_thinking,
            BodyState::Speaking => o.speed_speaking,
            _ => 1.0,
        };
        let ex = &self.extras;
        let t = self.t;
        for p in &mut self.organelles {
            p.a += p.s * dt * ex.organelle_angular_gain * sp;
            p.r += (t * ex.organelle_radial_rate + p.a * ex.organelle_radial_phase_gain).sin()
                * dt
                * ex.organelle_radial_gain;
            p.r = p.r.clamp(ex.organelle_r_min, ex.organelle_r_max);
        }
    }

    fn step_nucleus(&mut self, dt: f64) {
        let c0 = W_REF / 2.0;
        let (mut nx, mut ny) = (c0, c0);
        if let Some(m) = self.pointer {
            let dx = m[0] - c0;
            let dy = m[1] - c0;
            let dist = {
                let d = dx.hypot(dy);
                if d == 0.0 { 1.0 } else { d }
            };
            let mm = (W_REF * self.params.nucleus_follow_max)
                .min(dist * self.extras.nucleus_follow_gain);
            nx += dx / dist * mm;
            ny += dy / dist * mm;
        }
        // 0.08 per 60 Hz frame, applied frame-rate independently.
        let lerp = 1.0 - (1.0 - self.params.physics.nucleus_follow_lerp).powf(dt * 60.0);
        self.nucleus[0] += (nx - self.nucleus[0]) * lerp;
        self.nucleus[1] += (ny - self.nucleus[1]) * lerp;
    }
}

/// Hover radius in reference units: 34, but never smaller than the minimum hit radius
/// (12 logical px) on a small canvas.
pub(crate) fn hover_radius(ex: &Extras, p: &Params, canvas_px: f32) -> f64 {
    let scale = (f64::from(canvas_px) / W_REF).max(1e-3);
    ex.hover_radius.max(p.scaling.min_hit_radius_px / scale)
}

/// Stable seed for per-cell randomness outside parity mode.
#[allow(dead_code)]
pub(crate) fn id_seed(id: &str) -> u32 {
    (hash_unit(id) * 4_294_967_296.0) as u32
}
