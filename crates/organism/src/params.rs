//! Simulation parameters: [`Params`] (every number comes from the generated
//! [`crate::tokens`], i.e. from `design-system/tokens.json`) and [`Extras`] (constants that
//! live in the reference simulation but not in the tokens).
//!
//! The simulation runs in a *reference space* of [`W_REF`] units wide, so every pixel
//! constant is literal; `Scene` scales the result to the real canvas size.

use crate::tokens as tk;

/// Width of the reference canvas in simulation units (`organism.html` default).
pub const W_REF: f64 = 640.0;

/// Widen an `f32` token to the `f64` the author wrote (`0.13f32` -> `0.13f64`, not
/// `0.1299999952...`), so golden parity holds to 1e-9.
#[must_use]
pub fn d(x: f32) -> f64 {
    x.to_string().parse().unwrap_or_else(|_| f64::from(x))
}

/// Per-state scalar (`idle, listening, thinking, speaking, delegating`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ByState {
    pub idle: f64,
    pub listening: f64,
    pub thinking: f64,
    pub speaking: f64,
    pub delegating: f64,
}

/// Membrane noise: `sum w_i sin(f_i a + g_i t + k c_i)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MembraneNoise {
    pub amplitude: f64,
    pub breath: f64,
    pub breath_rad_per_second: f64,
    /// `[angular frequency, time frequency, weight]`.
    pub harmonics: [[f64; 3]; 3],
    pub phase_multipliers: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceRipple {
    pub amplitude: f64,
    pub angular: [f64; 2],
    pub temporal: [f64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bulge {
    pub amount: f64,
    pub sigma: f64,
    pub dead_factor: f64,
    pub birth_extra: f64,
    pub birth_seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NucleusScale {
    pub amp_gain: f64,
    pub thinking_pulse_amount: f64,
    pub thinking_pulse_rad: f64,
    pub other_pulse_amount: f64,
    pub other_pulse_rad: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Organelles {
    pub count: usize,
    pub size_min: f64,
    pub size_max: f64,
    /// Speed multiplier for idle / thinking / speaking; every other state uses 1.
    pub speed_idle: f64,
    pub speed_thinking: f64,
    pub speed_speaking: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pseudopod {
    pub samples: usize,
    pub wobble: f64,
    pub wobble_dead: f64,
    pub wobble_voice_gain: f64,
    pub vesicles: usize,
    pub flow_working: f64,
    pub flow_waiting: f64,
    pub flow_thinking_multiplier: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellParams {
    pub radius: f64,
    pub hover_scale: f64,
    pub noise_working: f64,
    pub noise_other: f64,
    pub nucleus_ratio: f64,
    pub beat_rad: f64,
    pub beat_amount: f64,
    pub blink_rad: f64,
    pub blink_min_alpha: f64,
    pub tremor_rad: f64,
    pub tremor_amount: f64,
    pub tremor_nucleus_ratio: f64,
    pub pending_alpha: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub min_slots: usize,
    pub max_cells: usize,
    pub compact_after_seconds: f64,
    pub slot_offset: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scaling {
    pub reference_width: f64,
    pub min_stroke_device_px: f64,
    pub min_halo_px: f64,
    pub hide_labels_below: f64,
    pub min_hit_radius_px: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waves {
    pub speed: f64,
    pub decay_per_second: f64,
    pub speaking_rate_per_second: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Physics {
    pub cell_spring_k: f64,
    pub cell_spring_k_newborn: f64,
    pub cell_damping_per_second: f64,
    pub cell_position_gain: f64,
    pub nucleus_follow_lerp: f64,
    pub amplitude_lerp_per_second: f64,
    pub hover_lerp_per_second: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Durations {
    pub cell_birth: f64,
    pub wave_life: f64,
    pub cell_death: f64,
    pub remove_terminal_after: f64,
    pub delegating_overlay: f64,
}

/// Idle / thinking amplitude targets (`organism.voiceEnvelope`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Envelope {
    pub idle: f64,
    pub thinking: f64,
}

/// All parameters that come from the design tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    pub habitat_radius: f64,
    pub ring_radius: f64,
    pub body_radius: f64,
    pub body_radius_by_state: ByState,
    pub membrane: MembraneNoise,
    pub voice_ripple: VoiceRipple,
    pub bulge: Bulge,
    pub nucleus_radius: f64,
    pub nucleus_follow_max: f64,
    pub nucleus_scale: NucleusScale,
    pub organelles: Organelles,
    pub pseudopod: Pseudopod,
    pub cell: CellParams,
    pub ring: Ring,
    pub scaling: Scaling,
    pub envelope: Envelope,
    pub waves: Waves,
    pub physics: Physics,
    pub durations: Durations,
}

impl Default for Params {
    fn default() -> Self {
        Self::from_tokens()
    }
}

impl Params {
    /// Build the parameters from the generated tokens.
    #[must_use]
    pub fn from_tokens() -> Self {
        let h = tk::ORGANISM_MEMBRANE_NOISE_HARMONICS;
        let hd = |i: usize| [d(h[i][0]), d(h[i][1]), d(h[i][2])];
        let pm = tk::ORGANISM_MEMBRANE_NOISE_PHASE_MULTIPLIERS;
        Self {
            habitat_radius: d(tk::ORGANISM_HABITAT_RADIUS),
            ring_radius: d(tk::ORGANISM_RING_RADIUS),
            body_radius: d(tk::ORGANISM_BODY_RADIUS),
            body_radius_by_state: ByState {
                idle: d(tk::ORGANISM_BODY_RADIUS_BY_STATE_IDLE),
                listening: d(tk::ORGANISM_BODY_RADIUS_BY_STATE_LISTENING),
                thinking: d(tk::ORGANISM_BODY_RADIUS_BY_STATE_THINKING),
                speaking: d(tk::ORGANISM_BODY_RADIUS_BY_STATE_SPEAKING),
                delegating: d(tk::ORGANISM_BODY_RADIUS_BY_STATE_DELEGATING),
            },
            membrane: MembraneNoise {
                amplitude: d(tk::ORGANISM_MEMBRANE_NOISE_AMPLITUDE),
                breath: d(tk::ORGANISM_MEMBRANE_NOISE_BREATH),
                breath_rad_per_second: d(tk::ORGANISM_MEMBRANE_NOISE_BREATH_RAD_PER_SECOND),
                harmonics: [hd(0), hd(1), hd(2)],
                phase_multipliers: [d(pm[0]), d(pm[1]), d(pm[2])],
            },
            voice_ripple: VoiceRipple {
                amplitude: d(tk::ORGANISM_VOICE_RIPPLE_AMPLITUDE),
                angular: [
                    d(tk::ORGANISM_VOICE_RIPPLE_ANGULAR[0]),
                    d(tk::ORGANISM_VOICE_RIPPLE_ANGULAR[1]),
                ],
                temporal: [
                    d(tk::ORGANISM_VOICE_RIPPLE_TEMPORAL[0]),
                    d(tk::ORGANISM_VOICE_RIPPLE_TEMPORAL[1]),
                ],
            },
            bulge: Bulge {
                amount: d(tk::ORGANISM_BULGE_TOWARD_CELL_AMOUNT),
                sigma: d(tk::ORGANISM_BULGE_TOWARD_CELL_SIGMA),
                dead_factor: d(tk::ORGANISM_BULGE_TOWARD_CELL_DEAD_FACTOR),
                birth_extra: d(tk::ORGANISM_BULGE_TOWARD_CELL_BIRTH_EXTRA),
                birth_seconds: d(tk::ORGANISM_BULGE_TOWARD_CELL_BIRTH_SECONDS),
            },
            nucleus_radius: d(tk::ORGANISM_NUCLEUS_RADIUS),
            nucleus_follow_max: d(tk::ORGANISM_NUCLEUS_FOLLOW_MAX),
            nucleus_scale: NucleusScale {
                amp_gain: d(tk::ORGANISM_NUCLEUS_SCALE_AMP_GAIN),
                thinking_pulse_amount: d(tk::ORGANISM_NUCLEUS_SCALE_THINKING_PULSE_AMOUNT),
                thinking_pulse_rad: d(tk::ORGANISM_NUCLEUS_SCALE_THINKING_PULSE_RAD_PER_SECOND),
                other_pulse_amount: d(tk::ORGANISM_NUCLEUS_SCALE_OTHER_PULSE_AMOUNT),
                other_pulse_rad: d(tk::ORGANISM_NUCLEUS_SCALE_OTHER_PULSE_RAD_PER_SECOND),
            },
            organelles: Organelles {
                count: tk::ORGANISM_ORGANELLES_COUNT as usize,
                size_min: d(tk::ORGANISM_ORGANELLES_SIZE_MIN),
                size_max: d(tk::ORGANISM_ORGANELLES_SIZE_MAX),
                speed_idle: d(tk::ORGANISM_ORGANELLES_SPEED_BY_STATE_IDLE),
                speed_thinking: d(tk::ORGANISM_ORGANELLES_SPEED_BY_STATE_THINKING),
                speed_speaking: d(tk::ORGANISM_ORGANELLES_SPEED_BY_STATE_SPEAKING),
            },
            pseudopod: Pseudopod {
                samples: tk::ORGANISM_PSEUDOPOD_SAMPLES as usize,
                wobble: d(tk::ORGANISM_PSEUDOPOD_WOBBLE),
                wobble_dead: d(tk::ORGANISM_PSEUDOPOD_WOBBLE_DEAD),
                wobble_voice_gain: d(tk::ORGANISM_PSEUDOPOD_WOBBLE_VOICE_GAIN),
                vesicles: tk::ORGANISM_PSEUDOPOD_VESICLES as usize,
                flow_working: d(tk::ORGANISM_PSEUDOPOD_FLOW_SPEED_WORKING),
                flow_waiting: d(tk::ORGANISM_PSEUDOPOD_FLOW_SPEED_WAITING),
                flow_thinking_multiplier: d(tk::ORGANISM_PSEUDOPOD_FLOW_SPEED_THINKING_MULTIPLIER),
            },
            cell: CellParams {
                radius: d(tk::ORGANISM_CELL_RADIUS),
                hover_scale: d(tk::ORGANISM_CELL_HOVER_SCALE),
                noise_working: d(tk::ORGANISM_CELL_NOISE_WORKING),
                noise_other: d(tk::ORGANISM_CELL_NOISE_OTHER),
                nucleus_ratio: d(tk::ORGANISM_CELL_NUCLEUS_RATIO),
                beat_rad: d(tk::ORGANISM_CELL_BEAT_RAD_PER_SECOND),
                beat_amount: d(tk::ORGANISM_CELL_BEAT_AMOUNT),
                blink_rad: d(tk::ORGANISM_CELL_BLINK_RAD_PER_SECOND),
                blink_min_alpha: d(tk::ORGANISM_CELL_BLINK_MIN_ALPHA),
                tremor_rad: d(tk::ORGANISM_ERROR_TREMOR_RAD_PER_SECOND),
                tremor_amount: d(tk::ORGANISM_ERROR_TREMOR_AMOUNT),
                tremor_nucleus_ratio: d(tk::ORGANISM_ERROR_TREMOR_NUCLEUS_RATIO),
                pending_alpha: d(tk::ORGANISM_PENDING_ALPHA),
            },
            ring: Ring {
                min_slots: tk::ORGANISM_RING_MIN_SLOTS as usize,
                max_cells: tk::ORGANISM_RING_MAX_CELLS as usize,
                compact_after_seconds: d(tk::ORGANISM_RING_COMPACT_AFTER_SECONDS),
                slot_offset: d(tk::ORGANISM_RING_SLOT_OFFSET),
            },
            scaling: Scaling {
                reference_width: d(tk::ORGANISM_SCALING_REFERENCE_WIDTH),
                min_stroke_device_px: d(tk::ORGANISM_SCALING_MIN_STROKE_DEVICE_PX),
                min_halo_px: d(tk::ORGANISM_SCALING_MIN_HALO_PX),
                hide_labels_below: d(tk::ORGANISM_SCALING_HIDE_LABELS_BELOW),
                min_hit_radius_px: d(tk::ORGANISM_SCALING_MIN_HIT_RADIUS_PX),
            },
            envelope: Envelope {
                idle: d(tk::ORGANISM_VOICE_ENVELOPE_IDLE),
                thinking: d(tk::ORGANISM_VOICE_ENVELOPE_THINKING),
            },
            waves: Waves {
                speed: d(tk::ORGANISM_WAVES_SPEED),
                decay_per_second: d(tk::ORGANISM_WAVES_DECAY_PER_SECOND),
                speaking_rate_per_second: d(tk::ORGANISM_WAVES_SPEAKING_RATE_PER_SECOND),
            },
            physics: Physics {
                cell_spring_k: d(tk::PHYSICS_CELL_SPRING_K),
                cell_spring_k_newborn: d(tk::PHYSICS_CELL_SPRING_K_NEWBORN),
                cell_damping_per_second: d(tk::PHYSICS_CELL_DAMPING_PER_SECOND),
                cell_position_gain: d(tk::PHYSICS_CELL_POSITION_GAIN),
                nucleus_follow_lerp: d(tk::PHYSICS_NUCLEUS_FOLLOW_LERP),
                amplitude_lerp_per_second: d(tk::PHYSICS_AMPLITUDE_LERP_PER_SECOND),
                hover_lerp_per_second: d(tk::PHYSICS_HOVER_LERP_PER_SECOND),
            },
            durations: Durations {
                cell_birth: d(tk::DURATION_CELL_BIRTH),
                wave_life: d(tk::DURATION_WAVE_LIFE),
                cell_death: d(tk::DURATION_CELL_DEATH),
                remove_terminal_after: d(tk::DURATION_REMOVE_TERMINAL_AFTER),
                delegating_overlay: d(tk::DURATION_DELEGATING_OVERLAY),
            },
        }
    }
}

/// Constants hard-coded in `organism.html` that are not (yet) design tokens.
///
/// All lengths are in reference units (pixels of the 640 px reference canvas).
#[derive(Clone, Debug, PartialEq)]
pub struct Extras {
    /// `2*pi` as the reference writes it (`6.283`); used in parity mode only.
    pub tau_reference: f64,
    /// Cell phase multiplier of `Math.random()` (`6.28`).
    pub phase_span: f64,
    /// Organelle speed `0.2 + rand * 0.6`.
    pub organelle_speed_min: f64,
    pub organelle_speed_span: f64,
    /// Organelle angular speed factor (`0.7`), radial wobble rate (`0.9`, `3`, `0.08`).
    pub organelle_angular_gain: f64,
    pub organelle_radial_rate: f64,
    pub organelle_radial_phase_gain: f64,
    pub organelle_radial_gain: f64,
    pub organelle_r_min: f64,
    pub organelle_r_max: f64,
    /// Organelles are drawn at `bodyR * r * 0.92`.
    pub organelle_radius_factor: f64,
    /// Cell drift amplitude (px) and rates.
    pub drift_amplitude: f64,
    pub drift_rate: f64,
    pub drift_y_ratio: f64,
    /// `born` value of a cell that is not new (`-10`).
    pub born_old: f64,
    /// Radius growth time of a newborn (`min(1, age / 1.6)`).
    pub cell_growth_seconds: f64,
    pub hover_radius: f64,
    pub click_radius: f64,
    /// Waves.
    pub wave_alpha: f64,
    pub wave_width_base: f64,
    /// Habitat ring alpha / width.
    pub habitat_alpha: f64,
    /// Body: fill alpha, halo width/alpha, crisp width/alpha, polyline segments (high LOD).
    pub body_fill_alpha: f64,
    pub body_halo_width: f64,
    pub body_halo_alpha: f64,
    pub body_stroke_width: f64,
    pub body_stroke_alpha: f64,
    pub body_samples: usize,
    pub body_samples_low: usize,
    /// Pseudopod.
    pub pseudopod_halo_width: f64,
    pub pseudopod_halo_alpha: f64,
    pub pseudopod_halo_alpha_dead: f64,
    pub pseudopod_core_width: f64,
    pub pseudopod_core_alpha: f64,
    pub pseudopod_core_alpha_dead: f64,
    pub pseudopod_cli_alpha: f64,
    pub pseudopod_dash: [f64; 2],
    pub pseudopod_start_inset: f64,
    pub pseudopod_end_inset: f64,
    /// Vesicles.
    pub vesicle_radius_base: f64,
    pub vesicle_radius_gain: f64,
    /// Nucleus drawing.
    pub nucleus_orbit_factor: f64,
    pub nucleus_corona_factor: f64,
    pub nucleus_dash: [f64; 2],
    pub nucleus_dash_speed: f64,
    pub nucleus_follow_gain: f64,
    pub satellite_radius: f64,
    pub satellite_rad_per_second: f64,
    pub satellite_spacing: f64,
    /// Cell drawing.
    pub cell_samples: usize,
    pub cell_samples_low: usize,
    pub cell_noise_time: f64,
    pub cell_dim_alpha: f64,
    pub cell_dash: [f64; 2],
    pub cell_nucleus_stroke_width: f64,
    pub selection_gap: f64,
    pub selection_wobble: f64,
    pub selection_rad_per_second: f64,
    pub badge_radius: f64,
    pub badge_offset: f64,
    pub label_gap: f64,
    pub label_size: f64,
    pub label_weight: u16,
    pub badge_size: f64,
    pub badge_weight: u16,
    pub badge_baseline: f64,
    pub organelles_low: usize,
    /// Delegating overlay body pulse (fraction of base radius).
    pub overlay_pulse: f64,
}

// The reference literally writes 6.283 / 6.28; parity needs those exact values.
#[allow(clippy::approx_constant)]
impl Default for Extras {
    fn default() -> Self {
        Self {
            tau_reference: 6.283,
            phase_span: 6.28,
            organelle_speed_min: 0.2,
            organelle_speed_span: 0.6,
            organelle_angular_gain: 0.7,
            organelle_radial_rate: 0.9,
            organelle_radial_phase_gain: 3.0,
            organelle_radial_gain: 0.08,
            organelle_r_min: 0.05,
            organelle_r_max: 0.95,
            organelle_radius_factor: 0.92,
            drift_amplitude: 6.0,
            drift_rate: 0.6,
            drift_y_ratio: 0.6,
            born_old: -10.0,
            cell_growth_seconds: 1.6,
            hover_radius: 34.0,
            click_radius: 40.0,
            wave_alpha: 0.45,
            wave_width_base: 1.0,
            habitat_alpha: 0.07,
            body_fill_alpha: 0.06,
            body_halo_width: 7.0,
            body_halo_alpha: 0.16,
            body_stroke_width: 1.5,
            body_stroke_alpha: 0.9,
            body_samples: 160,
            body_samples_low: 96,
            pseudopod_halo_width: 9.0,
            pseudopod_halo_alpha: 0.14,
            pseudopod_halo_alpha_dead: 0.07,
            pseudopod_core_width: 1.6,
            pseudopod_core_alpha: 0.75,
            pseudopod_core_alpha_dead: 0.3,
            pseudopod_cli_alpha: 0.5,
            pseudopod_dash: [3.0, 5.0],
            pseudopod_start_inset: 2.0,
            pseudopod_end_inset: 1.0,
            vesicle_radius_base: 2.0,
            vesicle_radius_gain: 1.2,
            nucleus_orbit_factor: 1.9,
            nucleus_corona_factor: 1.35,
            nucleus_dash: [3.0, 4.0],
            nucleus_dash_speed: 20.0,
            nucleus_follow_gain: 0.12,
            satellite_radius: 2.0,
            satellite_rad_per_second: 2.4,
            satellite_spacing: 2.094,
            cell_samples: 48,
            cell_samples_low: 32,
            cell_noise_time: 1.4,
            cell_dim_alpha: 0.35,
            cell_dash: [3.0, 4.0],
            cell_nucleus_stroke_width: 1.5,
            selection_gap: 7.0,
            selection_wobble: 1.5,
            selection_rad_per_second: 2.0,
            badge_radius: 7.0,
            badge_offset: 0.75,
            label_gap: 16.0,
            label_size: 11.0,
            label_weight: 300,
            badge_size: 9.0,
            badge_weight: 400,
            badge_baseline: 3.0,
            organelles_low: 30,
            overlay_pulse: 0.03,
        }
    }
}
