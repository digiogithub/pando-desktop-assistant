//! The renderer-neutral draw list produced by [`crate::Organism::shape`].
//!
//! Prims are in paint order. Colours have `globalAlpha` already folded in. Coordinates are in
//! logical pixels of the canvas (the [`crate::Scene`] scale is applied).

use std::ops::Range;
use std::sync::Arc;

use crate::tokens::Color;

/// Straight-alpha colour, components in `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    #[must_use]
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// From a design token (`0xRRGGBB` plus alpha byte).
    #[must_use]
    pub fn from_token(c: Color) -> Self {
        Self {
            r: ((c.rgb >> 16) & 0xFF) as f32 / 255.0,
            g: ((c.rgb >> 8) & 0xFF) as f32 / 255.0,
            b: (c.rgb & 0xFF) as f32 / 255.0,
            a: f32::from(c.a) / 255.0,
        }
    }

    /// Replace alpha.
    #[must_use]
    pub fn with_alpha(self, a: f64) -> Self {
        Self {
            a: a as f32,
            ..self
        }
    }

    /// Multiply alpha (`globalAlpha`).
    #[must_use]
    pub fn mul_alpha(self, m: f64) -> Self {
        Self {
            a: self.a * m as f32,
            ..self
        }
    }

    /// `[r, g, b]` as 0-255 integers.
    #[must_use]
    pub fn rgb8(self) -> [u8; 3] {
        let q = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        [q(self.r), q(self.g), q(self.b)]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cap {
    #[default]
    Butt,
    Round,
}

/// `on` / `off` lengths and the phase (offset along the pattern), in logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dash {
    pub on: f32,
    pub off: f32,
    pub phase: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Rgba,
    pub cap: Cap,
    pub dash: Option<Dash>,
}

impl Stroke {
    #[must_use]
    pub fn solid(width: f32, color: Rgba) -> Self {
        Self {
            width,
            color,
            cap: Cap::Butt,
            dash: None,
        }
    }
}

/// Text to draw; the label is shared with the input so no string is allocated per frame.
#[derive(Clone, Debug, PartialEq)]
pub enum TextContent {
    Static(&'static str),
    Shared(Arc<str>),
}

impl TextContent {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            TextContent::Static(s) => s,
            TextContent::Shared(s) => s,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    /// A circle, filled and/or stroked (fill first).
    Circle {
        c: [f32; 2],
        r: f32,
        fill: Option<Rgba>,
        stroke: Option<Stroke>,
    },
    /// Polyline over `DrawList::points[pts]`: fill first, then each stroke in order.
    Poly {
        pts: Range<u32>,
        closed: bool,
        fill: Option<Rgba>,
        strokes: [Option<Stroke>; 2],
    },
    /// Pseudopod: points are the *samples*; the renderer draws the quadratic spline through
    /// them with mid-point joins (`moveTo p0; quadTo(p_i, mid(p_i, p_i+1)); lineTo p_N`).
    Curve {
        pts: Range<u32>,
        stroke: Stroke,
    },
    /// Many small discs of one colour: `DrawList::discs[discs]` as `[x, y, r]`.
    Discs {
        discs: Range<u32>,
        color: Rgba,
    },
    /// Centre-aligned text; `pos` is the baseline centre.
    Text {
        text: TextContent,
        pos: [f32; 2],
        size: f32,
        weight: u16,
        color: Rgba,
    },
    /// Following prims are clipped to the closed polygon `points[pts]` until `ClipEnd`.
    ClipBegin {
        pts: Range<u32>,
    },
    ClipEnd,
}

/// Paint-ordered primitives plus the point pools they index.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawList {
    pub points: Vec<[f32; 2]>,
    pub discs: Vec<[f32; 3]>,
    pub prims: Vec<Prim>,
}

impl DrawList {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty the list, keeping the buffers.
    pub fn clear(&mut self) {
        self.points.clear();
        self.discs.clear();
        self.prims.clear();
    }

    pub(crate) fn begin_pts(&self) -> u32 {
        self.points.len() as u32
    }

    pub(crate) fn range_from(&self, start: u32) -> Range<u32> {
        start..self.points.len() as u32
    }

    /// Points of a range.
    #[must_use]
    pub fn pts(&self, r: &Range<u32>) -> &[[f32; 2]] {
        &self.points[r.start as usize..r.end as usize]
    }
}
