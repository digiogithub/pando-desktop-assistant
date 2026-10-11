//! Crate-side dashing with a phase. GPUI's `dash_array` has no offset, so renderers use these
//! helpers to get identical dashes (including the nucleus orbit's moving phase) everywhere.

use std::ops::Range;

use crate::draw::{Cap, Dash, DrawList, Prim, Stroke};

/// Split the polyline `pts` (optionally closed) into "on" runs of `dash`, appending the run
/// points to `out_pts` and one range per run to `out_runs` (ranges index `out_pts`).
pub fn dash_polyline(
    pts: &[[f32; 2]],
    closed: bool,
    dash: Dash,
    out_pts: &mut Vec<[f32; 2]>,
    out_runs: &mut Vec<Range<u32>>,
) {
    let period = dash.on + dash.off;
    if pts.len() < 2 || dash.on.is_nan() || dash.on <= 0.0 || !period.is_finite() {
        return;
    }
    if dash.off <= 0.0 {
        let start = out_pts.len() as u32;
        out_pts.extend_from_slice(pts);
        if closed {
            out_pts.push(pts[0]);
        }
        out_runs.push(start..out_pts.len() as u32);
        return;
    }
    // Position inside the pattern at the start of the path.
    let pos = (dash.phase % period + period) % period;
    let mut on = pos < dash.on;
    let mut run_start: Option<u32> = None;
    let n = if closed { pts.len() } else { pts.len() - 1 };
    let mut remaining_in_phase = if on { dash.on - pos } else { period - pos };
    if on {
        run_start = Some(out_pts.len() as u32);
        out_pts.push(pts[0]);
    }
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let seg = (b[0] - a[0]).hypot(b[1] - a[1]);
        if seg <= 0.0 {
            continue;
        }
        let mut consumed = 0.0_f32;
        while seg - consumed > remaining_in_phase {
            consumed += remaining_in_phase;
            let u = consumed / seg;
            let p = [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u];
            if on {
                out_pts.push(p);
                if let Some(s) = run_start.take() {
                    out_runs.push(s..out_pts.len() as u32);
                }
                remaining_in_phase = dash.off;
            } else {
                run_start = Some(out_pts.len() as u32);
                out_pts.push(p);
                remaining_in_phase = dash.on;
            }
            on = !on;
            if remaining_in_phase <= 0.0 {
                break;
            }
        }
        remaining_in_phase -= seg - consumed;
        if on {
            out_pts.push(b);
        }
    }
    if on && let Some(s) = run_start.take() {
        out_runs.push(s..out_pts.len() as u32);
    }
}

/// A circle as a closed polygon with `segments` edges (the first point is repeated at the end).
pub fn circle_polyline(c: [f32; 2], r: f32, segments: usize, out: &mut Vec<[f32; 2]>) {
    let n = segments.max(3);
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        out.push([c[0] + a.cos() * r, c[1] + a.sin() * r]);
    }
}

/// Flatten the pseudopod spline (quadratic through mid-points) with `per_segment` steps.
pub fn flatten_curve(samples: &[[f32; 2]], per_segment: usize, out: &mut Vec<[f32; 2]>) {
    if samples.len() < 3 {
        out.extend_from_slice(samples);
        return;
    }
    let steps = per_segment.max(1);
    let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    out.push(samples[0]);
    let mut cur = samples[0];
    for i in 1..samples.len() - 1 {
        let ctrl = samples[i];
        let end = mid(samples[i], samples[i + 1]);
        for s in 1..=steps {
            let t = s as f32 / steps as f32;
            let u = 1.0 - t;
            out.push([
                u * u * cur[0] + 2.0 * u * t * ctrl[0] + t * t * end[0],
                u * u * cur[1] + 2.0 * u * t * ctrl[1] + t * t * end[1],
            ]);
        }
        cur = end;
    }
    out.push(samples[samples.len() - 1]);
}

/// Replace every dashed stroke of `list` with open solid runs, so renderers without dash
/// phase support can draw the list as is. Fills stay where they are.
#[must_use]
pub fn resolve_dashes(list: &DrawList) -> DrawList {
    let mut out = DrawList::new();
    out.discs.clone_from(&list.discs);
    let mut tmp = Vec::new();
    let mut runs = Vec::new();
    let mut emit_runs =
        |out: &mut DrawList, pts: &[[f32; 2]], closed: bool, stroke: Stroke, dash: Dash| {
            tmp.clear();
            runs.clear();
            dash_polyline(pts, closed, dash, &mut tmp, &mut runs);
            let solid = Stroke {
                dash: None,
                cap: if stroke.cap == Cap::Butt {
                    Cap::Butt
                } else {
                    stroke.cap
                },
                ..stroke
            };
            for r in &runs {
                let start = out.begin_pts();
                out.points
                    .extend_from_slice(&tmp[r.start as usize..r.end as usize]);
                let range = out.range_from(start);
                out.prims.push(Prim::Poly {
                    pts: range,
                    closed: false,
                    fill: None,
                    strokes: [Some(solid), None],
                });
            }
        };
    for prim in &list.prims {
        match prim {
            Prim::Poly {
                pts,
                closed,
                fill,
                strokes,
            } if strokes.iter().flatten().any(|s| s.dash.is_some()) => {
                let src = list.pts(pts);
                if fill.is_some() {
                    let start = out.begin_pts();
                    out.points.extend_from_slice(src);
                    let range = out.range_from(start);
                    out.prims.push(Prim::Poly {
                        pts: range,
                        closed: *closed,
                        fill: *fill,
                        strokes: [None, None],
                    });
                }
                for s in strokes.iter().flatten() {
                    match s.dash {
                        Some(d) => emit_runs(&mut out, src, *closed, *s, d),
                        None => {
                            let start = out.begin_pts();
                            out.points.extend_from_slice(src);
                            let range = out.range_from(start);
                            out.prims.push(Prim::Poly {
                                pts: range,
                                closed: *closed,
                                fill: None,
                                strokes: [Some(*s), None],
                            });
                        }
                    }
                }
            }
            Prim::Circle {
                c,
                r,
                fill,
                stroke: Some(s),
            } if s.dash.is_some() => {
                if fill.is_some() {
                    out.prims.push(Prim::Circle {
                        c: *c,
                        r: *r,
                        fill: *fill,
                        stroke: None,
                    });
                }
                let mut poly = Vec::new();
                circle_polyline(*c, *r, 64, &mut poly);
                if let Some(d) = s.dash {
                    emit_runs(&mut out, &poly, false, *s, d);
                }
            }
            Prim::Curve { pts, stroke } if stroke.dash.is_some() => {
                let mut poly = Vec::new();
                flatten_curve(list.pts(pts), 6, &mut poly);
                if let Some(d) = stroke.dash {
                    emit_runs(&mut out, &poly, false, *stroke, d);
                }
            }
            other => {
                // Re-home point ranges of untouched prims.
                match other {
                    Prim::Poly {
                        pts,
                        closed,
                        fill,
                        strokes,
                    } => {
                        let start = out.begin_pts();
                        out.points.extend_from_slice(list.pts(pts));
                        let range = out.range_from(start);
                        out.prims.push(Prim::Poly {
                            pts: range,
                            closed: *closed,
                            fill: *fill,
                            strokes: *strokes,
                        });
                    }
                    Prim::Curve { pts, stroke } => {
                        let start = out.begin_pts();
                        out.points.extend_from_slice(list.pts(pts));
                        let range = out.range_from(start);
                        out.prims.push(Prim::Curve {
                            pts: range,
                            stroke: *stroke,
                        });
                    }
                    Prim::ClipBegin { pts } => {
                        let start = out.begin_pts();
                        out.points.extend_from_slice(list.pts(pts));
                        let range = out.range_from(start);
                        out.prims.push(Prim::ClipBegin { pts: range });
                    }
                    p => out.prims.push(p.clone()),
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn length(p: &[[f32; 2]]) -> f32 {
        p.windows(2)
            .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
            .sum()
    }

    #[test]
    fn straight_line_dashes_have_expected_lengths() {
        let pts = [[0.0, 0.0], [100.0, 0.0]];
        let mut out = Vec::new();
        let mut runs = Vec::new();
        dash_polyline(
            &pts,
            false,
            Dash {
                on: 3.0,
                off: 4.0,
                phase: 0.0,
            },
            &mut out,
            &mut runs,
        );
        // 100 / 7 = 14 full periods + 2 px of the next dash.
        assert_eq!(runs.len(), 15);
        let total: f32 = runs
            .iter()
            .map(|r| length(&out[r.start as usize..r.end as usize]))
            .sum();
        assert!((total - (14.0 * 3.0 + 2.0)).abs() < 1e-3, "{total}");
        assert!((out[0][0]).abs() < 1e-6 && (out[1][0] - 3.0).abs() < 1e-5);
    }

    #[test]
    fn phase_shifts_the_pattern() {
        let pts = [[0.0, 0.0], [20.0, 0.0]];
        let mut out = Vec::new();
        let mut runs = Vec::new();
        // Phase 5 starts inside the gap (on 3 / off 4): first dash begins at x = 2.
        dash_polyline(
            &pts,
            false,
            Dash {
                on: 3.0,
                off: 4.0,
                phase: 5.0,
            },
            &mut out,
            &mut runs,
        );
        let first = &out[runs[0].start as usize..runs[0].end as usize];
        assert!((first[0][0] - 2.0).abs() < 1e-5, "{first:?}");
        assert!((first[first.len() - 1][0] - 5.0).abs() < 1e-5);
        // Negative phase is the same as phase + period.
        let mut out2 = Vec::new();
        let mut runs2 = Vec::new();
        dash_polyline(
            &pts,
            false,
            Dash {
                on: 3.0,
                off: 4.0,
                phase: -2.0,
            },
            &mut out2,
            &mut runs2,
        );
        assert_eq!(out, out2);
    }

    #[test]
    fn dashes_follow_corners_and_closed_paths() {
        let sq = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let mut out = Vec::new();
        let mut runs = Vec::new();
        dash_polyline(
            &sq,
            true,
            Dash {
                on: 5.0,
                off: 5.0,
                phase: 0.0,
            },
            &mut out,
            &mut runs,
        );
        let total: f32 = runs
            .iter()
            .map(|r| length(&out[r.start as usize..r.end as usize]))
            .sum();
        assert!((total - 20.0).abs() < 1e-3, "{total}");
    }

    #[test]
    fn resolve_dashes_keeps_total_on_length() {
        let mut list = DrawList::new();
        list.points.extend([[0.0, 0.0], [70.0, 0.0]]);
        let dashed = Stroke {
            dash: Some(Dash {
                on: 3.0,
                off: 4.0,
                phase: 0.0,
            }),
            ..Stroke::solid(1.0, crate::draw::Rgba::new(1.0, 1.0, 1.0, 1.0))
        };
        list.prims.push(Prim::Poly {
            pts: 0..2,
            closed: false,
            fill: None,
            strokes: [Some(dashed), None],
        });
        let r = resolve_dashes(&list);
        assert_eq!(r.prims.len(), 10);
        assert!(
            r.prims.iter().all(
                |p| matches!(p, Prim::Poly { strokes: [Some(s), None], .. } if s.dash.is_none())
            )
        );
    }
}
