//! [`Organism::shape`]: turn the simulation state into a [`DrawList`], in the draw order of the
//! reference (habitat, waves, pseudopods, body, organelles, nucleus, cells).

use crate::draw::{Cap, Dash, DrawList, Prim, Rgba, Stroke, TextContent};
use crate::layout::Scene;
use crate::noise::noise;
use crate::params::W_REF;
use crate::sim::{CellSim, Organism};
use crate::tokens::{self, Membrane};
use crate::types::{BodyState, Engine, Status};

const HALF: f64 = W_REF / 2.0;

struct Painter<'a> {
    o: &'a Organism,
    sc: &'a Scene,
    s: f64,
    out: &'a mut DrawList,
    mar: Rgba,
    gold: Rgba,
    blue: Rgba,
    red: Rgba,
    bosque: Rgba,
}

impl Painter<'_> {
    fn pt(&self, x: f64, y: f64) -> [f32; 2] {
        [(x * self.s) as f32, (y * self.s) as f32]
    }

    fn len(&self, v: f64) -> f32 {
        (v * self.s) as f32
    }

    fn engine_color(&self, e: Engine) -> Rgba {
        match e {
            Engine::Pando => self.gold,
            Engine::ClaudeAcp | Engine::ClaudeExternal => self.blue,
            Engine::Cli => self.mar,
        }
    }

    fn dash(&self, pattern: [f64; 2], phase: f64) -> Dash {
        Dash {
            on: self.len(pattern[0]),
            off: self.len(pattern[1]),
            phase: self.len(phase),
        }
    }

    fn circle(&mut self, x: f64, y: f64, r: f64, fill: Option<Rgba>, stroke: Option<Stroke>) {
        let c = self.pt(x, y);
        let r = self.len(r);
        self.out.prims.push(Prim::Circle { c, r, fill, stroke });
    }
}

impl Organism {
    /// Build the draw list for `scene`.
    #[must_use]
    pub fn shape(&self, scene: &Scene) -> DrawList {
        let mut out = DrawList::new();
        self.shape_into(scene, &mut out);
        out
    }

    /// Like [`shape`](Self::shape) but reusing `out`'s buffers (no per-frame allocation once warm).
    pub fn shape_into(&self, scene: &Scene, out: &mut DrawList) {
        out.clear();
        let mut p = Painter {
            o: self,
            sc: scene,
            s: f64::from(scene.scale),
            out,
            mar: Rgba::from_token(tokens::MARFIL),
            gold: Rgba::from_token(tokens::ALAMO),
            blue: Rgba::from_token(tokens::CLAUDE_BLUE),
            red: Rgba::from_token(tokens::BERMELLON),
            bosque: Rgba::from_token(tokens::BOSQUE),
        };
        p.habitat_and_waves();
        for c in &self.cells {
            p.pseudopod(c);
        }
        p.body_and_organelles();
        p.nucleus();
        for c in &self.cells {
            p.cell(c);
        }
    }
}

impl Painter<'_> {
    fn habitat_and_waves(&mut self) {
        let o = self.o;
        let ex = &o.extras;
        let st = Stroke::solid(
            self.sc.stroke_width(1.0),
            self.mar.with_alpha(ex.habitat_alpha),
        );
        self.circle(HALF, HALF, W_REF * o.params.habitat_radius, None, Some(st));
        let base = W_REF * o.params.body_radius;
        for w in &o.waves {
            let st = Stroke::solid(
                self.sc.stroke_width(ex.wave_width_base + w.life),
                self.mar.with_alpha(ex.wave_alpha * w.life),
            );
            self.circle(HALF, HALF, base + w.r, None, Some(st));
        }
    }

    fn pseudopod(&mut self, c: &CellSim) {
        let o = self.o;
        let (p, ex) = (&o.params, &o.extras);
        let a = &c.input;
        let dd = a.status.is_dead();
        let fade = o.cell_fade(c);
        let retract = o.cell_retract(c);
        if retract <= 1e-3 || fade <= 0.0 {
            return;
        }
        let t = o.t;
        let rm = o.reduced_motion;
        let ang = (c.spring.y - HALF).atan2(c.spring.x - HALF);
        let (ca, sa) = (ang.cos(), ang.sin());
        let r0 = o.body_radius(ang) - ex.pseudopod_start_inset;
        let (x0, y0) = (HALF + ca * r0, HALF + sa * r0);
        let cr = o.cell_radius(c);
        let ex1 = c.spring.x - ca * (cr - ex.pseudopod_end_inset);
        let ey1 = c.spring.y - sa * (cr - ex.pseudopod_end_inset);
        let (x1, y1) = (x0 + (ex1 - x0) * retract, y0 + (ey1 - y0) * retract);
        let len = (x1 - x0).hypot(y1 - y0);
        let (nx, ny) = if len > 1e-9 {
            (-(y1 - y0) / len, (x1 - x0) / len)
        } else {
            (0.0, 0.0)
        };
        let wob = if rm {
            0.0
        } else {
            (if dd {
                p.pseudopod.wobble_dead
            } else {
                p.pseudopod.wobble
            }) * (1.0 + o.amp.value * p.pseudopod.wobble_voice_gain)
        };
        let n = p.pseudopod.samples;
        let speed = if dd { 0.6 } else { 2.4 };
        let start = self.out.begin_pts();
        let mut samples = [[0.0_f64; 2]; 64];
        let n = n.min(samples.len() - 1);
        for (i, slot) in samples.iter_mut().enumerate().take(n + 1) {
            let s = i as f64 / n as f64;
            let env = (s * std::f64::consts::PI).sin();
            let off = env
                * wob
                * ((s * 9.0 - t * speed + c.phase).sin()
                    + 0.5 * (s * 17.0 + t * 3.1 + c.phase).sin());
            *slot = [x0 + (x1 - x0) * s + nx * off, y0 + (y1 - y0) * s + ny * off];
            let q = self.pt(slot[0], slot[1]);
            self.out.points.push(q);
        }
        let range = self.out.range_from(start);
        let cl = self.engine_color(a.engine);
        let halo = Stroke {
            width: self.sc.halo_width(ex.pseudopod_halo_width),
            color: self
                .mar
                .with_alpha(if dd {
                    ex.pseudopod_halo_alpha_dead
                } else {
                    ex.pseudopod_halo_alpha
                })
                .mul_alpha(fade),
            cap: Cap::Round,
            dash: None,
        };
        let core_alpha = if dd {
            ex.pseudopod_core_alpha_dead
        } else {
            ex.pseudopod_core_alpha
        };
        let cli = a.engine == Engine::Cli;
        let core_color = if cli {
            self.mar.with_alpha(ex.pseudopod_cli_alpha)
        } else {
            cl
        };
        let core = Stroke {
            width: self.sc.stroke_width(ex.pseudopod_core_width),
            color: core_color.mul_alpha(core_alpha * fade),
            cap: Cap::Round,
            dash: cli.then(|| self.dash(ex.pseudopod_dash, 0.0)),
        };
        self.out.prims.push(Prim::Curve {
            pts: range.clone(),
            stroke: halo,
        });
        self.out.prims.push(Prim::Curve {
            pts: range,
            stroke: core,
        });
        if !dd {
            let ves_color = if a.status == Status::Waiting {
                self.mar
            } else {
                cl
            };
            for &f in &c.flow {
                let i = ((f * n as f64).floor() as usize).min(n - 1);
                let (pp, qq) = (samples[i], samples[i + 1]);
                let u = f * n as f64 - i as f64;
                let sinf = (f * std::f64::consts::PI).sin();
                let fill = ves_color.mul_alpha((0.5 + 0.5 * sinf) * fade);
                self.circle(
                    pp[0] + (qq[0] - pp[0]) * u,
                    pp[1] + (qq[1] - pp[1]) * u,
                    ex.vesicle_radius_base + sinf * ex.vesicle_radius_gain,
                    Some(fill),
                    None,
                );
            }
        }
    }

    fn body_and_organelles(&mut self) {
        let o = self.o;
        let ex = &o.extras;
        let tau = o.tau();
        let samples = if self.sc.parity {
            ex.body_samples
        } else {
            self.sc.lod.body_samples
        };
        let start = self.out.begin_pts();
        for i in 0..=samples {
            let a = i as f64 / samples as f64 * tau;
            let r = o.body_radius(a);
            let q = self.pt(HALF + a.cos() * r, HALF + a.sin() * r);
            self.out.points.push(q);
        }
        let body = self.out.range_from(start);
        let halo = Stroke::solid(
            self.sc.halo_width(ex.body_halo_width),
            self.mar.with_alpha(ex.body_halo_alpha),
        );
        let crisp = Stroke::solid(
            self.sc.stroke_width(ex.body_stroke_width),
            self.mar.mul_alpha(ex.body_stroke_alpha),
        );
        self.out.prims.push(Prim::Poly {
            pts: body.clone(),
            closed: true,
            fill: Some(self.mar.with_alpha(ex.body_fill_alpha)),
            strokes: [Some(halo), Some(crisp)],
        });

        let count = if self.sc.parity {
            o.organelles.len()
        } else {
            self.sc.lod.organelles.min(o.organelles.len())
        };
        let parts = &o.organelles[..count];
        if self.sc.parity {
            self.out.prims.push(Prim::ClipBegin { pts: body });
            for q in parts {
                let rr = o.body_radius(q.a) * q.r * ex.organelle_radius_factor;
                let fill = self.mar.with_alpha(0.18 + 0.25 * q.r);
                self.circle(
                    HALF + q.a.cos() * rr,
                    HALF + q.a.sin() * rr,
                    q.sz,
                    Some(fill),
                    None,
                );
            }
            self.out.prims.push(Prim::ClipEnd);
        } else {
            // Analytic clip: keep the whole disc inside the membrane, then batch by alpha.
            const BUCKETS: usize = 8;
            let mut placed = [[0.0_f32; 3]; 128];
            let mut bucket = [0_u8; 128];
            let m = parts.len().min(128);
            for (i, q) in parts.iter().take(m).enumerate() {
                let br = o.body_radius(q.a) * ex.organelle_radius_factor;
                let rr = (br * q.r).min((br - q.sz).max(0.0));
                let [x, y] = self.pt(HALF + q.a.cos() * rr, HALF + q.a.sin() * rr);
                placed[i] = [x, y, self.len(q.sz)];
                bucket[i] = (q.r.clamp(0.0, 1.0) * (BUCKETS - 1) as f64).round() as u8;
            }
            for b in 0..BUCKETS {
                let from = self.out.discs.len() as u32;
                for i in 0..m {
                    if usize::from(bucket[i]) == b {
                        self.out.discs.push(placed[i]);
                    }
                }
                let to = self.out.discs.len() as u32;
                if to > from {
                    let r = b as f64 / (BUCKETS - 1) as f64;
                    self.out.prims.push(Prim::Discs {
                        discs: from..to,
                        color: self.mar.with_alpha(0.18 + 0.25 * r),
                    });
                }
            }
        }
    }

    fn nucleus(&mut self) {
        let o = self.o;
        let (p, ex) = (&o.params, &o.extras);
        let t = o.t;
        let rm = o.reduced_motion;
        let state = o.visual_state();
        let thinking = state == BodyState::Thinking;
        let ns = p.nucleus_scale;
        let pulse = if rm {
            0.0
        } else if thinking {
            ns.thinking_pulse_amount * (t * ns.thinking_pulse_rad).sin()
        } else {
            ns.other_pulse_amount * (t * ns.other_pulse_rad).sin()
        };
        let nr = W_REF * p.nucleus_radius * (1.0 + o.amp.value * ns.amp_gain + pulse);
        let [nx, ny] = o.nucleus;
        let gold = self.gold;
        let orbit = Stroke {
            width: self.sc.stroke_width(1.0),
            color: gold.with_alpha(0.35),
            cap: Cap::Butt,
            dash: thinking.then(|| {
                self.dash(
                    ex.nucleus_dash,
                    if rm { 0.0 } else { -t * ex.nucleus_dash_speed },
                )
            }),
        };
        let orbit_r = nr * ex.nucleus_orbit_factor;
        // The reference sets lineDashOffset on every orbit; keep the offset even when solid.
        self.circle(nx, ny, orbit_r, None, Some(orbit));
        self.circle(
            nx,
            ny,
            nr * ex.nucleus_corona_factor,
            Some(gold.with_alpha(0.18)),
            None,
        );
        self.circle(nx, ny, nr, Some(gold), None);
        if thinking {
            for k in 0..3 {
                let spin = if rm {
                    0.0
                } else {
                    t * ex.satellite_rad_per_second
                };
                let aa = spin + f64::from(k) * ex.satellite_spacing;
                self.circle(
                    nx + aa.cos() * orbit_r,
                    ny + aa.sin() * orbit_r,
                    ex.satellite_radius,
                    Some(self.mar),
                    None,
                );
            }
        }
    }

    fn cell(&mut self, c: &CellSim) {
        let o = self.o;
        let (p, ex) = (&o.params, &o.extras);
        let a = &c.input;
        let t = o.t;
        let rm = o.reduced_motion;
        let fade = o.cell_fade(c);
        if fade <= 0.0 {
            return;
        }
        let cr = o.cell_radius(c);
        let cl = self.engine_color(a.engine);
        let paused = a.status == Status::Paused;
        let dim = if paused { ex.cell_dim_alpha } else { 1.0 } * fade;
        let tau = o.tau();
        let samples = if self.sc.parity {
            ex.cell_samples
        } else {
            self.sc.lod.cell_samples
        };
        let wig = if rm {
            0.0
        } else if a.status == Status::Working {
            p.cell.noise_working
        } else {
            p.cell.noise_other
        };
        let start = self.out.begin_pts();
        for i in 0..=samples {
            let aa = i as f64 / samples as f64 * tau;
            let r = cr * (1.0 + wig * noise(&p.membrane, aa, t * ex.cell_noise_time, c.phase));
            let q = self.pt(c.spring.x + aa.cos() * r, c.spring.y + aa.sin() * r);
            self.out.points.push(q);
        }
        let range = self.out.range_from(start);
        let (fill, stroke) = if a.engine == Engine::Cli {
            (
                self.mar.with_alpha(0.04),
                Stroke {
                    width: self.sc.stroke_width(1.2),
                    color: self.mar.with_alpha(0.75).mul_alpha(dim),
                    cap: Cap::Butt,
                    dash: Some(self.dash(ex.cell_dash, 0.0)),
                },
            )
        } else {
            (
                self.mar.with_alpha(0.07),
                Stroke::solid(self.sc.stroke_width(1.3), self.mar.mul_alpha(dim)),
            )
        };
        debug_assert!(
            matches!(a.engine.token().membrane, Membrane::Dashed) == (a.engine == Engine::Cli)
        );
        self.out.prims.push(Prim::Poly {
            pts: range,
            closed: true,
            fill: Some(fill.mul_alpha(dim)),
            strokes: [Some(stroke), None],
        });

        // Nucleus.
        let beat = if a.status == Status::Working && !rm {
            1.0 + p.cell.beat_amount * (t * p.cell.beat_rad + c.phase).sin().max(0.0).powi(3)
        } else {
            1.0
        };
        let mut nrr = cr * p.cell.nucleus_ratio * beat;
        let mut fill_c = cl;
        let mut al = 1.0;
        if a.status == Status::Waiting {
            al = if rm {
                p.cell.blink_min_alpha + (1.0 - p.cell.blink_min_alpha) * 0.5
            } else {
                p.cell.blink_min_alpha
                    + (1.0 - p.cell.blink_min_alpha)
                        * if (t * p.cell.blink_rad).sin() > 0.0 {
                            1.0
                        } else {
                            0.0
                        }
            };
        }
        if a.status == Status::Done {
            fill_c = self.mar.with_alpha(0.45);
        }
        if a.status == Status::Error {
            fill_c = self.red;
            let tremor = if rm {
                0.0
            } else {
                (t * p.cell.tremor_rad).sin()
            };
            nrr = cr * p.cell.tremor_nucleus_ratio * (1.0 + p.cell.tremor_amount * tremor);
        }
        let na = dim * al;
        let hollow = matches!(a.engine, Engine::ClaudeExternal | Engine::Cli);
        if hollow {
            let st = Stroke::solid(
                self.sc.stroke_width(ex.cell_nucleus_stroke_width),
                fill_c.mul_alpha(na),
            );
            self.circle(c.spring.x, c.spring.y, nrr, None, Some(st));
        } else {
            self.circle(
                c.spring.x,
                c.spring.y,
                nrr,
                Some(fill_c.mul_alpha(na)),
                None,
            );
        }

        // Selection ring.
        if o.selected.as_ref() == Some(&a.id) {
            let wobble = if rm {
                0.0
            } else {
                (t * ex.selection_rad_per_second).sin() * ex.selection_wobble
            };
            let st = Stroke::solid(
                self.sc.stroke_width(1.0),
                self.gold.with_alpha(0.75).mul_alpha(fade),
            );
            self.circle(
                c.spring.x,
                c.spring.y,
                cr + ex.selection_gap + wobble,
                None,
                Some(st),
            );
        }

        if !self.sc.lod.labels && !self.sc.parity {
            return;
        }
        // Badge.
        let badge = match a.status {
            Status::Done => Some("\u{2713}"),
            Status::Waiting => Some("?"),
            Status::Error => Some("!"),
            Status::Paused => Some("II"),
            _ => None,
        };
        if let Some(glyph) = badge.filter(|_| a.id.as_str() != crate::FOLD_ID) {
            let (bx, by) = (
                c.spring.x + cr * ex.badge_offset,
                c.spring.y - cr * ex.badge_offset,
            );
            let col = if a.status == Status::Error {
                self.red
            } else {
                self.mar
            };
            let st = Stroke::solid(self.sc.stroke_width(1.0), col.mul_alpha(fade));
            self.circle(
                bx,
                by,
                ex.badge_radius,
                Some(self.bosque.mul_alpha(fade)),
                Some(st),
            );
            self.out.prims.push(Prim::Text {
                text: TextContent::Static(glyph),
                pos: self.pt(bx, by + ex.badge_baseline),
                size: self.len(ex.badge_size),
                weight: ex.badge_weight,
                color: col.mul_alpha(fade),
            });
        }
        // Label.
        self.out.prims.push(Prim::Text {
            text: TextContent::Shared(a.label.clone()),
            pos: self.pt(c.spring.x, c.spring.y + cr + ex.label_gap),
            size: self.len(ex.label_size),
            weight: ex.label_weight,
            color: self.mar.with_alpha(0.55 + 0.45 * c.hover).mul_alpha(fade),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Config;
    use crate::types::{CellInput, Inputs};

    #[test]
    fn zero_length_pseudopod_does_not_produce_nan() {
        let cs = [CellInput::new("a", Engine::Pando, Status::Working)];
        let mut org = Organism::new(Config::default());
        for _ in 0..600 {
            org.step(1.0 / 60.0, &Inputs::new(BodyState::Idle, &cs));
        }
        // Park the cell exactly where the pseudopod's two ends coincide (angle 0).
        let r0 = org.body_radius(0.0) - org.extras.pseudopod_start_inset;
        let cr = org.cell_radius(&org.cells[0]);
        org.cells[0].spring.x = HALF + r0 + cr - org.extras.pseudopod_end_inset;
        org.cells[0].spring.y = HALF;
        let list = org.shape(&Scene::layout(640.0, 1.0));
        assert!(
            list.points
                .iter()
                .all(|p| p[0].is_finite() && p[1].is_finite())
        );
    }

    #[test]
    fn organelles_stay_inside_the_membrane_when_clipped_analytically() {
        let cs = [CellInput::new("a", Engine::Cli, Status::Done)];
        let mut org = Organism::new(Config::default());
        for _ in 0..200 {
            org.step(1.0 / 60.0, &Inputs::new(BodyState::Speaking, &cs));
        }
        let list = org.shape(&Scene::layout(640.0, 1.0));
        let mut n = 0;
        for p in &list.prims {
            if let Prim::Discs { discs, .. } = p {
                for d in &list.discs[discs.start as usize..discs.end as usize] {
                    let ang = f64::from(d[1] - 320.0).atan2(f64::from(d[0] - 320.0));
                    let dist =
                        f64::from(d[0] - 320.0).hypot(f64::from(d[1] - 320.0)) + f64::from(d[2]);
                    assert!(dist <= org.body_radius(ang) + 1e-3);
                    n += 1;
                }
            }
        }
        assert_eq!(n, 70);
    }
}
