//! Translates a [`DrawList`] into GPUI paths and text (ADR-0006).
//!
//! All geometry becomes `paint_path` calls in draw-list order, followed by every text prim,
//! so GPUI batches the paths into one pass and the glyphs into another (T-0024 finding 2).
//! Clip prims are ignored: `shape()` clips the organelles analytically outside parity mode.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as _, Hasher as _};

use organism::DrawList;
use organism::Prim;
use organism::draw::{Cap, Rgba as DrawRgba, Stroke};

use super::geometry::dash_polyline;
use crate::fonts::FONT_MONO;
use crate::ui::paint::{
    Font, FontWeight, Path, PathBuilder, ShapedLine, SharedString, TextAlign, TextRun, stroke_style,
};
use crate::ui::{App, Bounds, Pixels, Point, Rgba, Window, point, px};

/// What one frame cost in draw calls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintCounts {
    pub paths: usize,
    pub texts: usize,
}

/// Cache of shaped labels keyed by (text, size, weight, quantised colour).
#[derive(Default)]
pub struct TextCache {
    lines: HashMap<u64, ShapedLine>,
}

impl std::fmt::Debug for TextCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextCache({})", self.lines.len())
    }
}

/// Entries after which the cache is dropped (colours of fading labels accumulate).
const CACHE_LIMIT: usize = 512;

fn quant_alpha(a: f32) -> u8 {
    (a.clamp(0.0, 1.0) * 31.0).round() as u8
}

fn text_key(text: &str, size: f32, weight: u16, c: &DrawRgba) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    size.to_bits().hash(&mut h);
    weight.hash(&mut h);
    let [r, g, b] = c.rgb8();
    (r, g, b, quant_alpha(c.a)).hash(&mut h);
    h.finish()
}

fn gpui_rgba(c: DrawRgba) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a.clamp(0.0, 1.0),
    }
}

/// Segments of a circle polygon for a radius in logical px.
fn circle_segments(r: f32) -> usize {
    ((r.sqrt() * 6.0) as usize).clamp(8, 48)
}

fn add_circle(b: &mut PathBuilder, o: Point<Pixels>, c: [f32; 2], r: f32) {
    let n = circle_segments(r);
    let at = |i: usize| {
        let a = std::f32::consts::TAU * i as f32 / n as f32;
        point(
            px(f32::from(o.x) + c[0] + r * a.cos()),
            px(f32::from(o.y) + c[1] + r * a.sin()),
        )
    };
    b.move_to(at(0));
    for i in 1..n {
        b.line_to(at(i));
    }
    b.close();
}

fn pt(o: Point<Pixels>, p: [f32; 2]) -> Point<Pixels> {
    point(px(f32::from(o.x) + p[0]), px(f32::from(o.y) + p[1]))
}

fn add_polyline(b: &mut PathBuilder, o: Point<Pixels>, pts: &[[f32; 2]], closed: bool) {
    let mut it = pts.iter();
    if let Some(first) = it.next() {
        b.move_to(pt(o, *first));
        for p in it {
            b.line_to(pt(o, *p));
        }
        if closed {
            b.close();
        }
    }
}

fn stroke_builder(s: &Stroke) -> PathBuilder {
    PathBuilder::stroke(px(s.width)).with_style(stroke_style(s.width, s.cap == Cap::Round))
}

fn paint(window: &mut Window, b: PathBuilder, color: DrawRgba, counts: &mut PaintCounts) {
    if color.a <= 0.0 {
        return;
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, gpui_rgba(color));
        counts.paths += 1;
    }
}

fn paint_built(window: &mut Window, path: Path<Pixels>, color: DrawRgba, c: &mut PaintCounts) {
    window.paint_path(path, gpui_rgba(color));
    c.paths += 1;
}

fn stroke_poly(
    window: &mut Window,
    o: Point<Pixels>,
    pts: &[[f32; 2]],
    closed: bool,
    s: &Stroke,
    counts: &mut PaintCounts,
) {
    if s.color.a <= 0.0 || s.width <= 0.0 || pts.len() < 2 {
        return;
    }
    let mut b = stroke_builder(s);
    match s.dash {
        Some(d) => {
            for piece in dash_polyline(pts, closed, d.on, d.off, d.phase) {
                add_polyline(&mut b, o, &piece, false);
            }
        }
        None => add_polyline(&mut b, o, pts, closed),
    }
    paint(window, b, s.color, counts);
}

/// Paints the geometry of `list` with its origin at `origin`; text is painted afterwards by
/// [`paint_text`].
pub fn paint_paths(window: &mut Window, origin: Point<Pixels>, list: &DrawList) -> PaintCounts {
    let mut counts = PaintCounts::default();
    for prim in &list.prims {
        match prim {
            Prim::Circle { c, r, fill, stroke } => {
                if let Some(f) = fill {
                    let mut b = PathBuilder::fill();
                    add_circle(&mut b, origin, *c, *r);
                    paint(window, b, *f, &mut counts);
                }
                if let Some(s) = stroke
                    && s.color.a > 0.0
                {
                    let mut b = stroke_builder(s);
                    if let Some(d) = s.dash {
                        let n = circle_segments(*r);
                        let ring: Vec<[f32; 2]> = (0..n)
                            .map(|i| {
                                let a = std::f32::consts::TAU * i as f32 / n as f32;
                                [c[0] + r * a.cos(), c[1] + r * a.sin()]
                            })
                            .collect();
                        for piece in dash_polyline(&ring, true, d.on, d.off, d.phase) {
                            add_polyline(&mut b, origin, &piece, false);
                        }
                    } else {
                        add_circle(&mut b, origin, *c, *r);
                    }
                    paint(window, b, s.color, &mut counts);
                }
            }
            Prim::Poly {
                pts,
                closed,
                fill,
                strokes,
            } => {
                let p = list.pts(pts);
                if let Some(f) = fill
                    && p.len() >= 3
                {
                    let mut b = PathBuilder::fill();
                    add_polyline(&mut b, origin, p, true);
                    paint(window, b, *f, &mut counts);
                }
                for s in strokes.iter().flatten() {
                    stroke_poly(window, origin, p, *closed, s, &mut counts);
                }
            }
            Prim::Curve { pts, stroke } => {
                let p = list.pts(pts);
                if p.len() < 2 || stroke.color.a <= 0.0 || stroke.width <= 0.0 {
                    continue;
                }
                if let Some(d) = stroke.dash {
                    // Dashing needs a flattened curve; the spline is sampled densely enough
                    // that its control polygon is a faithful stand-in.
                    let mut b = stroke_builder(stroke);
                    for piece in dash_polyline(p, false, d.on, d.off, d.phase) {
                        add_polyline(&mut b, origin, &piece, false);
                    }
                    paint(window, b, stroke.color, &mut counts);
                    continue;
                }
                let mut b = stroke_builder(stroke);
                b.move_to(pt(origin, p[0]));
                for i in 1..p.len() - 1 {
                    let mid = [(p[i][0] + p[i + 1][0]) / 2.0, (p[i][1] + p[i + 1][1]) / 2.0];
                    b.curve_to(pt(origin, mid), pt(origin, p[i]));
                }
                b.line_to(pt(origin, p[p.len() - 1]));
                if let Ok(path) = b.build() {
                    paint_built(window, path, stroke.color, &mut counts);
                }
            }
            Prim::Discs { discs, color } => {
                if color.a <= 0.0 {
                    continue;
                }
                let mut b = PathBuilder::fill();
                for d in &list.discs[discs.start as usize..discs.end as usize] {
                    add_circle(&mut b, origin, [d[0], d[1]], d[2]);
                }
                paint(window, b, *color, &mut counts);
            }
            Prim::Text { .. } | Prim::ClipBegin { .. } | Prim::ClipEnd => {}
        }
    }
    counts
}

/// Shapes (cached) and paints one centred line of text; `pos` is the baseline centre.
#[allow(clippy::too_many_arguments)]
fn paint_line(
    window: &mut Window,
    cx: &mut App,
    cache: &mut TextCache,
    s: &str,
    pos: Point<Pixels>,
    size: f32,
    weight: u16,
    color: &DrawRgba,
    left_aligned: bool,
) {
    let key = text_key(s, size, weight, color);
    let line = cache.lines.entry(key).or_insert_with(|| {
        let run = TextRun {
            len: s.len(),
            font: Font {
                family: FONT_MONO.into(),
                weight: FontWeight(f32::from(weight)),
                ..Font::default()
            },
            color: gpui_rgba(DrawRgba {
                a: f32::from(quant_alpha(color.a)) / 31.0,
                ..*color
            })
            .into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(SharedString::from(s.to_owned()), px(size), &[run], None)
    });
    let (ascent, descent, width) = (line.ascent, line.descent, line.width);
    let x = if left_aligned {
        f32::from(pos.x)
    } else {
        f32::from(pos.x) - f32::from(width) / 2.0
    };
    let top_left = point(px(x), px(f32::from(pos.y) - f32::from(ascent)));
    let _ = line.paint(
        top_left,
        ascent + descent,
        TextAlign::Left,
        None,
        window,
        cx,
    );
}

/// Paints every text prim of `list` (after the paths).
pub fn paint_text(
    window: &mut Window,
    cx: &mut App,
    origin: Point<Pixels>,
    list: &DrawList,
    cache: &mut TextCache,
    counts: &mut PaintCounts,
) {
    if cache.lines.len() > CACHE_LIMIT {
        cache.lines.clear();
    }
    for prim in &list.prims {
        let Prim::Text {
            text,
            pos,
            size,
            weight,
            color,
        } = prim
        else {
            continue;
        };
        let s = text.as_str();
        if color.a <= 0.0 || *size <= 0.0 || s.is_empty() {
            continue;
        }
        let at = point(
            px(f32::from(origin.x) + pos[0]),
            px(f32::from(origin.y) + pos[1]),
        );
        paint_line(window, cx, cache, s, at, *size, *weight, color, false);
        counts.texts += 1;
    }
}

/// Keyboard focus ring around the focused cell.
pub fn paint_focus_ring(
    window: &mut Window,
    origin: Point<Pixels>,
    core: &super::Core,
    counts: &mut PaintCounts,
) {
    let Some(id) = &core.focused else { return };
    let Some(s) = core.semantics.iter().find(|s| &s.id == id) else {
        return;
    };
    let c = s.bounds.centre();
    let r = s.bounds.w.max(s.bounds.h) / 2.0 + 5.0;
    let color = DrawRgba::from_token(organism::tokens::MARFIL).with_alpha(0.9);
    let mut b = PathBuilder::stroke(px(2.0)).with_style(stroke_style(2.0, true));
    add_circle(&mut b, origin, c, r);
    paint(window, b, color, counts);
}

/// Frame-time overlay in the top-left corner (PDA-T-0043).
pub fn paint_timings(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    stats: &super::FrameStats,
    cache: &mut TextCache,
) {
    let (i50, i95) = stats.interval_p50_p95();
    let (c50, c95) = stats.cpu_p50_p95();
    let lines = [
        format!(
            "{:.0} fps  frame p50 {i50:.1} / p95 {i95:.1} ms",
            stats.fps()
        ),
        format!("cpu p50 {c50:.2} / p95 {c95:.2} ms"),
        format!(
            "prims {}  pts {}  paths {}",
            stats.prims, stats.points, stats.paths
        ),
    ];
    let color = DrawRgba::from_token(organism::tokens::MARFIL).with_alpha(0.75);
    for (i, l) in lines.iter().enumerate() {
        let at = point(
            px(f32::from(bounds.origin.x) + 12.0),
            px(f32::from(bounds.origin.y) + 20.0 + 15.0 * i as f32),
        );
        paint_line(window, cx, cache, l, at, 11.0, 400, &color, true);
    }
}

/// Square canvas area centred in `bounds`.
#[must_use]
pub fn square_in(bounds: Bounds<Pixels>) -> (Point<Pixels>, f32) {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let side = w.min(h).max(1.0);
    (
        point(
            px(f32::from(bounds.origin.x) + (w - side) / 2.0),
            px(f32::from(bounds.origin.y) + (h - side) / 2.0),
        ),
        side,
    )
}
