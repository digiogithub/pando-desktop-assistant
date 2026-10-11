//! Deterministic randomness, bit-identical to the JS harness.

/// `mulberry32`, the exact generator used by `harness/harness.mjs` in place of `Math.random`.
#[derive(Clone, Debug)]
pub struct Mulberry32 {
    state: u32,
}

impl Mulberry32 {
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Next value in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let a = self.state;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// FNV-1a hash of a cell id, mapped to `[0, 1)`: the stable replacement for the reference's
/// random cell phase, so restarts and other windows agree.
#[must_use]
pub fn hash_unit(id: &str) -> f64 {
    let mut h: u32 = 0x811C_9DC5;
    for b in id.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    // Final avalanche (murmur3 fmix) so near ids do not give near phases.
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    f64::from(h) / 4_294_967_296.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values printed by `node` running the reference `mulberry32` for seeds 1..3.
    const JS: [(u32, [f64; 5]); 3] = [
        (
            1,
            [
                0.6270739405881613,
                0.002735721180215478,
                0.5274470399599522,
                0.9810509674716741,
                0.9683778982143849,
            ],
        ),
        (
            2,
            [
                0.7342509443406016,
                0.32499843230471015,
                0.28529605525545776,
                0.5379551574587822,
                0.8752879470121115,
            ],
        ),
        (
            3,
            [
                0.7202267837710679,
                0.03866216051392257,
                0.4561921926215291,
                0.07492800964973867,
                0.7630053898319602,
            ],
        ),
    ];

    #[test]
    fn sequence_equals_js_for_seeds_1_to_3() {
        for (seed, want) in JS {
            let mut r = Mulberry32::new(seed);
            for w in want {
                assert_eq!(r.next_f64(), w, "seed {seed}");
            }
        }
    }

    #[test]
    fn hash_is_stable_and_in_unit_range() {
        let a = hash_unit("refactor-gpui");
        assert_eq!(a, hash_unit("refactor-gpui"));
        assert!((0.0..1.0).contains(&a));
        assert_ne!(hash_unit("cell-1"), hash_unit("cell-2"));
    }
}
