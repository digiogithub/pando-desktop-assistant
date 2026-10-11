//! Cell position springs.

/// A 2-D point mass pulled to a target.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellSpring {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
}

impl CellSpring {
    #[must_use]
    pub fn at(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
        }
    }

    /// The reference semi-implicit Euler step (`organism.html` L74-76):
    /// `v += (target - x) k dt; v *= damping^dt; x += v dt gain`.
    pub fn step(&mut self, tx: f64, ty: f64, k: f64, damping: f64, gain: f64, dt: f64) {
        self.vx += (tx - self.x) * k * dt;
        self.vy += (ty - self.y) * k * dt;
        let damp = damping.powf(dt);
        self.vx *= damp;
        self.vy *= damp;
        self.x += self.vx * dt * gain;
        self.y += self.vy * dt * gain;
    }

    /// Reduced-motion step: exponential approach without overshoot (monotone), rate in 1/s.
    pub fn step_critical(&mut self, tx: f64, ty: f64, rate: f64, dt: f64) {
        let f = 1.0 - (-rate * dt).exp();
        self.x += (tx - self.x) * f;
        self.y += (ty - self.y) * f;
        self.vx = 0.0;
        self.vy = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_converges_to_target() {
        let mut s = CellSpring::at(0.0, 0.0);
        for _ in 0..2000 {
            s.step(100.0, -50.0, 3.5, 0.08, 4.0, 1.0 / 60.0);
        }
        assert!((s.x - 100.0).abs() < 1e-3 && (s.y + 50.0).abs() < 1e-3);
    }

    #[test]
    fn critical_is_monotone() {
        let mut s = CellSpring::at(0.0, 0.0);
        let mut last = 0.0;
        for _ in 0..600 {
            s.step_critical(100.0, 0.0, 6.0, 1.0 / 60.0);
            assert!(s.x >= last && s.x <= 100.0);
            last = s.x;
        }
        assert!((s.x - 100.0).abs() < 1e-3);
    }
}
