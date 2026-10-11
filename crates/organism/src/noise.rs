//! Membrane noise: a sum of sines with integer angular frequencies, so it is periodic in the
//! angle and closed shapes stay closed.

use crate::params::MembraneNoise;

/// `sum_i w_i * sin(f_i * a + g_i * t + k * c_i)` (reference `noise(a, t, k)`).
#[must_use]
pub fn noise(m: &MembraneNoise, a: f64, t: f64, k: f64) -> f64 {
    let mut s = 0.0;
    for (h, c) in m.harmonics.iter().zip(m.phase_multipliers) {
        s += h[2] * (h[0] * a + h[1] * t + k * c).sin();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Params;
    use std::f64::consts::TAU;

    #[test]
    fn periodic_in_two_pi() {
        let m = Params::default().membrane;
        for i in 0..50 {
            let a = f64::from(i) * 0.37;
            let t = f64::from(i) * 0.11;
            let d = noise(&m, a, t, 2.5) - noise(&m, a + TAU, t, 2.5);
            assert!(d.abs() < 1e-9, "{d}");
        }
    }

    #[test]
    fn bounded_by_weight_sum() {
        let m = Params::default().membrane;
        let max: f64 = m.harmonics.iter().map(|h| h[2].abs()).sum();
        for i in 0..200 {
            assert!(noise(&m, f64::from(i) * 0.1, f64::from(i) * 0.3, 1.0).abs() <= max + 1e-12);
        }
    }
}
