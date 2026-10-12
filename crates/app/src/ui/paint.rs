//! Painting facade for the organism canvas (PDA-T-0038): GPUI paths, text runs, window
//! visibility, cursors and focus, so `organism_view` never names the kit crate.

pub use gpui_kit::gpui::{
    CursorStyle, Font, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Path,
    PathBuilder, PathStyle, QuitMode, ShapedLine, SharedString, StrokeOptions, TextAlign, TextRun,
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

/// Raster images for the CPU fallback (PDA-T-0042).
#[cfg(feature = "raster")]
pub mod raster_image {
    use std::sync::Arc;

    use gpui_kit::gpui::{Bounds, Corners, Pixels, RenderImage, Window};

    /// A premultiplied BGRA frame uploaded to the GPU.
    pub struct Frame(Arc<RenderImage>);

    impl std::fmt::Debug for Frame {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Frame")
        }
    }

    impl Frame {
        /// Wraps premultiplied **RGBA** pixels (tiny-skia order); `None` when the size does
        /// not match the buffer. GPUI's image shader expects straight-alpha BGRA, so the
        /// colour is un-premultiplied here.
        #[must_use]
        pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Self> {
            let mut bgra = rgba.to_vec();
            for px in bgra.as_chunks_mut::<4>().0 {
                let a = u32::from(px[3]);
                if a > 0 && a < 255 {
                    for c in &mut px[..3] {
                        *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                    }
                }
                px.swap(0, 2);
            }
            let img = image::RgbaImage::from_raw(width, height, bgra)?;
            Some(Self(Arc::new(RenderImage::new(vec![image::Frame::new(
                img,
            )]))))
        }

        /// Paints the frame stretched over `bounds`.
        pub fn paint(&self, window: &mut Window, bounds: Bounds<Pixels>) {
            let _ =
                window.paint_image(bounds, bounds, Corners::default(), self.0.clone(), 0, false);
        }

        /// Releases the GPU copy (call before replacing the frame).
        pub fn drop_from(&self, window: &mut Window) {
            let _ = window.drop_image(self.0.clone());
        }
    }
}

/// Window kinds for the floating mini window (PDA-T-0118, spike T-0025).
pub mod mini_window {
    pub use gpui_kit::gpui::layer_shell::{
        Anchor, KeyboardInteractivity, Layer, LayerShellOptions,
    };
    pub use gpui_kit::gpui::{WindowBackgroundAppearance, WindowKind};
}

/// Builds an action from its registered name (user keymap entries).
pub fn build_action(cx: &gpui_kit::App, name: &str) -> Result<Box<dyn gpui_kit::Action>, String> {
    cx.build_action(name, None).map_err(|e| e.to_string())
}
