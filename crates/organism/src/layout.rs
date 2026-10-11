//! Canvas layout: scale, minimum widths and level of detail.

use crate::params::{Params, W_REF};

/// Level of detail chosen from the canvas size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lod {
    pub body_samples: usize,
    pub cell_samples: usize,
    pub organelles: usize,
    /// Labels and badges are hidden on small canvases (mini window).
    pub labels: bool,
}

/// How the simulation maps to a canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    /// Canvas width in logical px.
    pub size_px: f32,
    /// Device pixels per logical px.
    pub scale_factor: f32,
    /// `size_px / 640`: every reference length is multiplied by this.
    pub scale: f32,
    pub lod: Lod,
    /// Minimum stroke width in logical px (`0.75` device px).
    pub min_stroke: f32,
    /// Minimum halo width in logical px (`2` device px).
    pub min_halo: f32,
    /// Exact reference output: per-disc circles, exact organelle clip, full sampling.
    pub parity: bool,
}

impl Scene {
    /// Layout for a canvas of `size_px` logical px at `scale_factor`.
    #[must_use]
    pub fn layout(size_px: f32, scale_factor: f32) -> Self {
        Self::layout_with(&Params::default(), size_px, scale_factor)
    }

    #[must_use]
    pub fn layout_with(p: &Params, size_px: f32, scale_factor: f32) -> Self {
        let sf = if scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        let size = size_px.max(1.0);
        let low = f64::from(size) < p.scaling.reference_width / 2.0;
        let lod = if low {
            Lod {
                body_samples: 96,
                cell_samples: 32,
                organelles: 30,
                labels: false,
            }
        } else {
            Lod {
                body_samples: 160,
                cell_samples: 48,
                organelles: p.organelles.count,
                labels: true,
            }
        };
        let labels = f64::from(size) >= p.scaling.hide_labels_below;
        Self {
            size_px: size,
            scale_factor: sf,
            scale: size / W_REF as f32,
            lod: Lod { labels, ..lod },
            min_stroke: p.scaling.min_stroke_device_px as f32 / sf,
            min_halo: p.scaling.min_halo_px as f32 / sf,
            parity: false,
        }
    }

    /// The 640 px, 1x scene the goldens use: exact reference output.
    #[must_use]
    pub fn reference() -> Self {
        Self {
            parity: true,
            ..Self::layout(W_REF as f32, 1.0)
        }
    }

    /// Scaled stroke width with the device-pixel floor.
    #[must_use]
    pub fn stroke_width(&self, w_ref: f64) -> f32 {
        (w_ref as f32 * self.scale).max(self.min_stroke)
    }

    /// Scaled halo width with the (larger) halo floor.
    #[must_use]
    pub fn halo_width(&self, w_ref: f64) -> f32 {
        (w_ref as f32 * self.scale).max(self.min_halo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_window_hides_labels_and_floors_strokes() {
        let s = Scene::layout(118.0, 1.0);
        assert!(!s.lod.labels);
        assert_eq!(
            (s.lod.body_samples, s.lod.cell_samples, s.lod.organelles),
            (96, 32, 30)
        );
        assert!((s.scale - 118.0 / 640.0).abs() < 1e-6);
        assert_eq!(s.stroke_width(1.0), 0.75);
        assert_eq!(s.halo_width(7.0), 2.0);
    }

    #[test]
    fn full_size_keeps_reference_values() {
        let s = Scene::layout(640.0, 2.0);
        assert!(s.lod.labels);
        assert_eq!(
            (s.lod.body_samples, s.lod.cell_samples, s.lod.organelles),
            (160, 48, 70)
        );
        assert_eq!(s.scale, 1.0);
        assert_eq!(s.stroke_width(1.5), 1.5);
        assert_eq!(s.min_stroke, 0.375);
    }
}
