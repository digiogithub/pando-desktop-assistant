//! Synthetic organism-like scene: closed membranes, pseudopods, halos, dashed rings.
//! Deterministic (xorshift) so every run draws the same geometry.

/// A polyline in a 1000x1000 canvas.
#[derive(Clone)]
pub struct Poly {
    pub pts: Vec<(f32, f32)>,
    pub closed: bool,
    pub width: f32,
    /// RGBA 0..=255.
    pub rgba: [u8; 4],
    pub dash: Option<(f32, f32)>,
    /// Halo: drawn first, wider and fainter.
    pub halo: Option<(f32, u8)>,
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f32 / (1u64 << 53) as f32
    }
}

/// `scale` multiplies the number of polylines (1 = ~100 paths, ~2.4k points).
pub fn build(scale: usize, t: f32) -> Vec<Poly> {
    let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);
    let mut out = Vec::new();
    let marfil = [0xF4, 0xF1, 0xE8, 255];
    let alamo = [0xE9, 0xB9, 0x49, 255];
    // Habitat ring (dashed) + orbit ring.
    out.push(Poly {
        pts: circle(500.0, 500.0, 470.0, 96, 0.0, 0.0),
        closed: true,
        width: 1.0,
        rgba: [0xF4, 0xF1, 0xE8, 18],
        dash: Some((6.0, 8.0)),
        halo: None,
    });
    // Mother cell: wobbling membrane with halo.
    out.push(Poly {
        pts: circle(500.0, 500.0, 130.0, 120, 6.0, t),
        closed: true,
        width: 1.5,
        rgba: marfil,
        dash: None,
        halo: Some((7.0, 30)),
    });
    for k in 0..(48 * scale) {
        let a = k as f32 / (48 * scale) as f32 * std::f32::consts::TAU;
        let (cx, cy) = (500.0 + 360.0 * a.cos(), 500.0 + 360.0 * a.sin());
        let r = 25.0 + rng.next() * 15.0;
        // Daughter cell membrane (~24 points).
        out.push(Poly {
            pts: circle(cx, cy, r, 24, 2.0, t + k as f32),
            closed: true,
            width: 1.3,
            rgba: if k % 3 == 0 { alamo } else { marfil },
            dash: None,
            halo: Some((5.0, 28)),
        });
        // Pseudopod (~24 points curved line to the mother).
        let mut pts = Vec::new();
        for i in 0..24 {
            let u = i as f32 / 23.0;
            let bend = (u * std::f32::consts::PI).sin() * 30.0 * (t + k as f32).sin();
            pts.push((
                cx + (500.0 - cx) * u + bend * a.sin(),
                cy + (500.0 - cy) * u - bend * a.cos(),
            ));
        }
        out.push(Poly {
            pts,
            closed: false,
            width: 1.6,
            rgba: [0xE9, 0xB9, 0x49, 160],
            dash: if k % 4 == 0 { Some((4.0, 5.0)) } else { None },
            halo: Some((9.0, 22)),
        });
    }
    out
}

fn circle(cx: f32, cy: f32, r: f32, n: usize, wobble: f32, t: f32) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let rr = r + wobble * (a * 3.0 + t).sin();
            (cx + rr * a.cos(), cy + rr * a.sin())
        })
        .collect()
}

pub fn point_count(polys: &[Poly]) -> usize {
    polys.iter().map(|p| p.pts.len()).sum()
}
