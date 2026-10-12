//! CPU raster backend of the organism (PDA-T-0042): the same [`DrawList`] drawn with
//! tiny-skia into a pixmap. Used as a runtime fallback when GPUI reports a software renderer
//! (T-0024 decision), and headless for visual verification (PDA-T-0041). Geometry only: text
//! prims are drawn by GPUI on top.
//!
//! Unlike the GPUI path backend, tiny-skia honours `ClipBegin` / `ClipEnd` exactly.

use organism::draw::{Cap, Dash, Rgba, Stroke};
use organism::{DrawList, Prim};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Stroke as SkStroke,
    StrokeDash, Transform,
};

/// Largest pixmap side used by the runtime fallback.
pub const MAX_FALLBACK_SIDE: u32 = 600;

fn color(c: Rgba) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a.clamp(0.0, 1.0)).unwrap_or(Color::TRANSPARENT)
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color(c));
    p.anti_alias = true;
    p
}

fn poly(pts: &[[f32; 2]], closed: bool) -> Option<Path> {
    let mut b = PathBuilder::new();
    let (first, rest) = pts.split_first()?;
    b.move_to(first[0], first[1]);
    for p in rest {
        b.line_to(p[0], p[1]);
    }
    if closed {
        b.close();
    }
    b.finish()
}

fn spline(pts: &[[f32; 2]]) -> Option<Path> {
    if pts.len() < 2 {
        return None;
    }
    let mut b = PathBuilder::new();
    b.move_to(pts[0][0], pts[0][1]);
    for i in 1..pts.len() - 1 {
        let mid = [
            (pts[i][0] + pts[i + 1][0]) / 2.0,
            (pts[i][1] + pts[i + 1][1]) / 2.0,
        ];
        b.quad_to(pts[i][0], pts[i][1], mid[0], mid[1]);
    }
    let last = pts[pts.len() - 1];
    b.line_to(last[0], last[1]);
    b.finish()
}

fn sk_stroke(s: &Stroke) -> SkStroke {
    let mut st = SkStroke {
        width: s.width,
        line_cap: if s.cap == Cap::Round {
            LineCap::Round
        } else {
            LineCap::Butt
        },
        line_join: LineJoin::Round,
        ..SkStroke::default()
    };
    if let Some(Dash { on, off, phase }) = s.dash {
        st.dash = StrokeDash::new(vec![on, off], phase);
    }
    st
}

struct Ctx<'a> {
    pm: &'a mut Pixmap,
    ts: Transform,
    mask: Option<Mask>,
}

impl Ctx<'_> {
    fn fill(&mut self, path: &Path, c: Rgba) {
        if c.a > 0.0 {
            self.pm.fill_path(
                path,
                &paint(c),
                FillRule::Winding,
                self.ts,
                self.mask.as_ref(),
            );
        }
    }

    fn stroke(&mut self, path: &Path, s: &Stroke) {
        if s.color.a > 0.0 && s.width > 0.0 {
            self.pm.stroke_path(
                path,
                &paint(s.color),
                &sk_stroke(s),
                self.ts,
                self.mask.as_ref(),
            );
        }
    }
}

/// Draws the geometry of `list` into `pm`. `scale` maps logical px to pixmap px.
pub fn draw(list: &DrawList, scale: f32, pm: &mut Pixmap) {
    let mut cx = Ctx {
        pm,
        ts: Transform::from_scale(scale, scale),
        mask: None,
    };
    let (w, h) = (cx.pm.width(), cx.pm.height());
    for prim in &list.prims {
        match prim {
            Prim::Circle { c, r, fill, stroke } => {
                let Some(path) = PathBuilder::from_circle(c[0], c[1], *r) else {
                    continue;
                };
                if let Some(f) = fill {
                    cx.fill(&path, *f);
                }
                if let Some(s) = stroke {
                    cx.stroke(&path, s);
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
                    && let Some(path) = poly(p, true)
                {
                    cx.fill(&path, *f);
                }
                if let Some(path) = poly(p, *closed) {
                    for s in strokes.iter().flatten() {
                        cx.stroke(&path, s);
                    }
                }
            }
            Prim::Curve { pts, stroke } => {
                if let Some(path) = spline(list.pts(pts)) {
                    cx.stroke(&path, stroke);
                }
            }
            Prim::Discs { discs, color } => {
                let mut b = PathBuilder::new();
                for d in &list.discs[discs.start as usize..discs.end as usize] {
                    b.push_circle(d[0], d[1], d[2]);
                }
                if let Some(path) = b.finish() {
                    cx.fill(&path, *color);
                }
            }
            Prim::ClipBegin { pts } => {
                cx.mask = poly(list.pts(pts), true).and_then(|path| {
                    let mut m = Mask::new(w, h)?;
                    m.fill_path(&path, FillRule::Winding, true, cx.ts);
                    Some(m)
                });
            }
            Prim::ClipEnd => cx.mask = None,
            Prim::Text { .. } => {}
        }
    }
}

/// Renders `list` (logical canvas of `side_px`) into a new square pixmap of `pixels` px with a
/// solid `background` (`None` = transparent).
#[must_use]
pub fn render(
    list: &DrawList,
    side_px: f32,
    pixels: u32,
    background: Option<Rgba>,
) -> Option<Pixmap> {
    let mut pm = Pixmap::new(pixels.max(1), pixels.max(1))?;
    if let Some(bg) = background {
        pm.fill(color(bg));
    }
    draw(list, pixels as f32 / side_px.max(1.0), &mut pm);
    Some(pm)
}

/// Pixmap side for the runtime fallback: the canvas in device px, capped at
/// [`MAX_FALLBACK_SIDE`].
#[must_use]
pub fn fallback_side(side_px: f32, scale_factor: f32) -> u32 {
    ((side_px * scale_factor.max(1.0)) as u32).clamp(64, MAX_FALLBACK_SIDE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use organism::{CellInput, Config, Engine, Inputs, Organism, Scene, Status};

    fn scene_list() -> (DrawList, Scene) {
        let mut o = Organism::new(Config::default());
        let cells = vec![
            CellInput::new("a", Engine::Pando, Status::Working),
            CellInput::new("b", Engine::Cli, Status::Waiting),
        ];
        let inputs = Inputs::new(organism::BodyState::Thinking, &cells);
        for _ in 0..90 {
            o.step(1.0 / 60.0, &inputs);
        }
        let scene = Scene::layout(400.0, 1.0);
        (o.shape(&scene), scene)
    }

    #[test]
    fn renders_a_non_empty_image_with_the_centre_inked() {
        let (list, scene) = scene_list();
        let bg = Rgba::from_token(organism::tokens::BOSQUE);
        let pm = render(&list, scene.size_px, 400, Some(bg)).expect("pixmap");
        assert_eq!(pm.width(), 400);
        let px = pm.pixel(200, 200).expect("pixel");
        // The nucleus is drawn over the background.
        assert!(px.red() > px.blue() || px.green() > 100, "{px:?}");
        let inked = pm
            .pixels()
            .iter()
            .filter(|p| p.demultiply().red() > 60)
            .count();
        assert!(inked > 500, "{inked}");
    }

    /// Writes the scene to `$PANDO_RASTER_DUMP` (a PNG path) for eyeballing.
    #[test]
    fn dump_png_when_requested() {
        let Ok(path) = std::env::var("PANDO_RASTER_DUMP") else {
            return;
        };
        let (list, scene) = scene_list();
        let bg = Rgba::from_token(organism::tokens::BOSQUE);
        let pm = render(&list, scene.size_px, 600, Some(bg)).expect("pixmap");
        pm.save_png(path).expect("png");
    }

    #[test]
    fn fallback_side_is_capped() {
        assert_eq!(fallback_side(900.0, 2.0), MAX_FALLBACK_SIDE);
        assert_eq!(fallback_side(300.0, 1.0), 300);
    }
}
