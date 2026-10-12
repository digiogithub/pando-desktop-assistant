//! Painting facade for the organism canvas (PDA-T-0038): GPUI paths, text runs, window
//! visibility, cursors and focus, so `organism_view` never names the kit crate.

pub use gpui_kit::gpui::{
    CursorStyle, Font, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Path,
    PathBuilder, PathStyle, ShapedLine, SharedString, StrokeOptions, TextAlign, TextRun,
    WindowVisibility, canvas,
};

use gpui_kit::gpui::{Pixels, px};

/// Stroke options for a polyline of `width` logical px with round caps and joins (`round`) or
/// the default butt caps.
#[must_use]
pub fn stroke_style(width: f32, round: bool) -> PathStyle {
    use lyon_tessellation::{LineCap, LineJoin};
    let mut options = StrokeOptions::default().with_line_width(width.max(0.01));
    if round {
        options = options
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
    }
    PathStyle::Stroke(options)
}

/// A pixel length.
#[must_use]
pub fn px_len(v: f32) -> Pixels {
    px(v)
}
