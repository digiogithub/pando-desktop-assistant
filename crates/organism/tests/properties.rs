//! Property tests: robustness of `step` + `shape` for arbitrary inputs, and numerical stability.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use organism::draw::{DrawList, Prim, Rgba, Stroke};
use organism::{BodyState, CellId, CellInput, Config, Engine, Inputs, Organism, Scene, Status};
use proptest::prelude::*;

const ENGINES: [Engine; 4] = [
    Engine::Pando,
    Engine::ClaudeAcp,
    Engine::ClaudeExternal,
    Engine::Cli,
];
const STATUSES: [Status; 6] = [
    Status::Pending,
    Status::Working,
    Status::Waiting,
    Status::Paused,
    Status::Done,
    Status::Error,
];

fn cells(spec: &[(usize, usize, bool)]) -> Vec<CellInput> {
    spec.iter()
        .enumerate()
        .map(|(i, &(e, s, new))| {
            CellInput::new(&format!("cell-{i}"), ENGINES[e % 4], STATUSES[s % 6]).with_new(new)
        })
        .collect()
}

fn finite(v: f32, what: &str) {
    assert!(v.is_finite(), "{what} is {v}");
}

fn finite_rgba(c: Rgba) {
    for v in [c.r, c.g, c.b, c.a] {
        finite(v, "colour");
    }
    assert!((0.0..=1.0).contains(&c.a), "alpha {}", c.a);
}

fn finite_stroke(s: &Stroke) {
    finite(s.width, "stroke width");
    assert!(s.width > 0.0);
    finite_rgba(s.color);
    if let Some(d) = s.dash {
        finite(d.on, "dash on");
        finite(d.off, "dash off");
        finite(d.phase, "dash phase");
    }
}

fn assert_list_finite(l: &DrawList) {
    for p in &l.points {
        finite(p[0], "x");
        finite(p[1], "y");
    }
    for d in &l.discs {
        d.iter().for_each(|v| finite(*v, "disc"));
    }
    for p in &l.prims {
        match p {
            Prim::Circle { c, r, fill, stroke } => {
                finite(c[0], "cx");
                finite(c[1], "cy");
                finite(*r, "r");
                assert!(*r >= 0.0);
                fill.iter().for_each(|f| finite_rgba(*f));
                stroke.iter().for_each(finite_stroke);
            }
            Prim::Poly {
                pts, fill, strokes, ..
            } => {
                assert!(pts.end as usize <= l.points.len() && pts.start <= pts.end);
                fill.iter().for_each(|f| finite_rgba(*f));
                strokes.iter().flatten().for_each(finite_stroke);
            }
            Prim::Curve { pts, stroke } => {
                assert!(pts.end as usize <= l.points.len());
                finite_stroke(stroke);
            }
            Prim::Discs { discs, color } => {
                assert!(discs.end as usize <= l.discs.len());
                finite_rgba(*color);
            }
            Prim::Text {
                pos, size, color, ..
            } => {
                finite(pos[0], "tx");
                finite(pos[1], "ty");
                finite(*size, "size");
                finite_rgba(*color);
            }
            Prim::ClipBegin { pts } => assert!(pts.end as usize <= l.points.len()),
            Prim::ClipEnd => {}
        }
    }
}

fn state_of(i: usize) -> BodyState {
    BodyState::ALL[i % 5]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 10_000, ..ProptestConfig::default() })]

    #[test]
    fn no_nan_and_invariants_hold(
        spec_a in prop::collection::vec((0usize..4, 0usize..6, any::<bool>()), 0..=32),
        spec_b in prop::collection::vec((0usize..4, 0usize..6, any::<bool>()), 0..=32),
        states in prop::collection::vec(0usize..5, 1..=4),
        dts in prop::collection::vec(0.0005f64..=0.05, 1..=6),
        pointer in prop::option::of((-0.5f64..1.5, -0.5f64..1.5)),
        mic in prop::option::of(0.0f32..=1.0),
        reduced in any::<bool>(),
        parity in any::<bool>(),
        size in 60.0f32..1200.0,
        selected in prop::option::of(0usize..32),
    ) {
        let (a, b) = (cells(&spec_a), cells(&spec_b));
        let mut org = Organism::new(Config { parity, seed: 7 });
        let scene = if parity { Scene::reference() } else { Scene::layout(size, 1.5) };
        let mut list = DrawList::new();
        for (i, dt) in dts.iter().enumerate() {
            let set = if i % 2 == 0 { &a } else { &b };
            let mut inputs = Inputs::new(state_of(states[i % states.len()]), set);
            inputs.pointer = pointer.map(|p| [p.0, p.1]);
            inputs.mic_rms = mic;
            inputs.tts_rms = mic;
            inputs.reduced_motion = reduced;
            inputs.canvas_px = size;
            inputs.selected = selected.map(|s| CellId::new(&format!("cell-{s}")));
            org.step(*dt, &inputs);
            org.shape_into(&scene, &mut list);
            assert_list_finite(&list);

            // Vesicle flow stays in [0, 1); waves are pruned; the radius is positive.
            for c in org.cells() {
                for f in &c.flow {
                    prop_assert!((0.0..1.0).contains(f), "flow {f}");
                }
                prop_assert!(c.spring.x.is_finite() && c.spring.y.is_finite());
            }
            for w in org.waves() {
                prop_assert!(w.life > 0.0 && w.life <= 1.0);
            }
            prop_assert!(org.waves().len() < 64);
            for k in 0..16 {
                let r = org.body_radius(f64::from(k) * 0.4);
                prop_assert!(r.is_finite() && r > 0.0, "body radius {r}");
            }
        }
    }
}

#[test]
fn body_polyline_is_closed_outside_parity_mode() {
    let spec: Vec<_> = (0..8).map(|i| (i, i, i % 3 == 0)).collect();
    let cs = cells(&spec);
    let mut org = Organism::new(Config::default());
    for s in 0..90 {
        let inputs = Inputs::new(state_of(s / 20), &cs);
        org.step(1.0 / 60.0, &inputs);
    }
    let list = org.shape(&Scene::layout(640.0, 1.0));
    let body = list
        .prims
        .iter()
        .find_map(|p| match p {
            Prim::Poly {
                pts,
                closed: true,
                fill: Some(_),
                strokes: [Some(h), _],
            } if h.width >= 2.0 => Some(pts.clone()),
            _ => None,
        })
        .expect("body polygon");
    let pts = list.pts(&body);
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    assert!(
        (first[0] - last[0]).abs() < 1e-3 && (first[1] - last[1]).abs() < 1e-3,
        "{first:?} vs {last:?}"
    );
}

fn settle(org: &mut Organism, cs: &[CellInput], state: BodyState, secs: f64, dt: f64) {
    let inputs = Inputs::new(state, cs);
    for _ in 0..(secs / dt) as usize {
        org.step(dt, &inputs);
    }
}

#[test]
fn settled_cells_stay_inside_the_habitat() {
    for n in [1usize, 6, 12, 32] {
        let spec: Vec<_> = (0..n).map(|i| (i, i, i % 4 == 0)).collect();
        let cs = cells(&spec);
        let mut org = Organism::new(Config::default());
        settle(&mut org, &cs, BodyState::Idle, 6.0, 1.0 / 60.0);
        let habitat = 640.0 * org.params().habitat_radius;
        for c in org.cells() {
            let d = (c.spring.x - 320.0).hypot(c.spring.y - 320.0);
            assert!(
                d + 20.0 < habitat,
                "n={n}: cell at {d} outside habitat {habitat}"
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    /// Two steps of dt agree with one step of 2dt within a pixel (settled, no random inputs).
    #[test]
    fn step_twice_is_close_to_step_double(
        spec in prop::collection::vec((0usize..4, 0usize..6, Just(false)), 1..=12),
        dt in 0.002f64..=0.025,
        state in prop::sample::select(vec![BodyState::Idle, BodyState::Listening, BodyState::Thinking]),
    ) {
        let cs = cells(&spec);
        let mut a = Organism::new(Config::default());
        settle(&mut a, &cs, state, 4.0, 1.0 / 60.0);
        let mut b = a.clone();
        let inputs = Inputs::new(state, &cs);
        for _ in 0..30 {
            a.step(dt, &inputs);
            a.step(dt, &inputs);
            b.step(2.0 * dt, &inputs);
        }
        for (x, y) in a.cells().iter().zip(b.cells()) {
            prop_assert!((x.spring.x - y.spring.x).abs() < 1.0 && (x.spring.y - y.spring.y).abs() < 1.0,
                "cell {:?}: {:?} vs {:?}", x.id(), x.spring, y.spring);
        }
        prop_assert!((a.nucleus()[0] - b.nucleus()[0]).abs() < 1e-9);
        prop_assert!((a.amplitude() - b.amplitude()).abs() < 0.05);
    }
}

#[test]
fn no_energy_blow_up_at_the_largest_dt() {
    let spec: Vec<_> = (0..32).map(|i| (i, i, i % 5 == 0)).collect();
    let cs = cells(&spec);
    let mut org = Organism::new(Config::default());
    let mut inputs = Inputs::new(BodyState::Thinking, &cs);
    inputs.pointer = Some([0.7, 0.2]);
    for i in 0..10_000 {
        inputs.state = state_of(i / 50);
        org.step(0.05, &inputs);
        if i % 500 == 0 {
            for c in org.cells() {
                assert!(
                    c.spring.x.abs() < 2000.0 && c.spring.y.abs() < 2000.0,
                    "step {i}: {:?}",
                    c.spring
                );
                assert!(
                    c.spring.vx.abs() < 1e4 && c.spring.vy.abs() < 1e4,
                    "step {i}: {:?}",
                    c.spring
                );
            }
        }
    }
    assert!(
        org.cells()
            .iter()
            .all(|c| c.spring.x.is_finite() && c.spring.vx.is_finite())
    );
    let list = org.shape(&Scene::layout(640.0, 1.0));
    assert_list_finite(&list);
}

#[test]
fn removing_every_cell_is_safe() {
    let spec: Vec<_> = (0..10).map(|i| (i, i, false)).collect();
    let cs = cells(&spec);
    let mut org = Organism::new(Config::default());
    settle(&mut org, &cs, BodyState::Speaking, 1.0, 1.0 / 60.0);
    for _ in 0..180 {
        org.step(1.0 / 60.0, &Inputs::new(BodyState::Idle, &[]));
        assert_list_finite(&org.shape(&Scene::layout(300.0, 2.0)));
    }
}
