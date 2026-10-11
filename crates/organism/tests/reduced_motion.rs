//! Reduced-motion profile and small-canvas layout (mini window).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use organism::draw::Prim;
use organism::{BodyState, CellInput, Config, DrawList, Engine, Inputs, Organism, Scene, Status};

const DT: f64 = 1.0 / 60.0;

fn cells() -> Vec<CellInput> {
    vec![
        CellInput::new("w", Engine::Pando, Status::Working),
        CellInput::new("q", Engine::ClaudeAcp, Status::Waiting),
        CellInput::new("e", Engine::Cli, Status::Error),
        CellInput::new("d", Engine::ClaudeExternal, Status::Done),
    ]
}

fn run(org: &mut Organism, cs: &[CellInput], state: BodyState, rm: bool, secs: f64) {
    let mut inputs = Inputs::new(state, cs);
    inputs.reduced_motion = rm;
    for _ in 0..(secs / DT).round() as usize {
        org.step(DT, &inputs);
    }
}

#[test]
fn breathing_noise_and_ripple_are_zero() {
    let mut org = Organism::new(Config::default());
    run(&mut org, &[], BodyState::Speaking, true, 2.0);
    let sample = |o: &Organism| {
        (0..32)
            .map(|k| o.body_radius(f64::from(k) * 0.2))
            .collect::<Vec<_>>()
    };
    let a = sample(&org);
    run(&mut org, &[], BodyState::Speaking, true, 0.77);
    let b = sample(&org);
    assert_eq!(a, b, "membrane must be static");
    let base = 640.0 * org.params().body_radius * org.params().body_radius_by_state.speaking;
    assert!(a.iter().all(|r| (r - base).abs() < 1e-9));

    // Without reduced motion the same scene moves.
    let mut live = Organism::new(Config::default());
    run(&mut live, &[], BodyState::Speaking, false, 2.0);
    let c = sample(&live);
    run(&mut live, &[], BodyState::Speaking, false, 0.77);
    assert_ne!(c, sample(&live));
}

#[test]
fn pseudopods_are_straight_and_organelles_static() {
    let cs = cells();
    let mut org = Organism::new(Config::default());
    run(&mut org, &cs, BodyState::Thinking, true, 3.0);
    let org_before: Vec<_> = org.organelles().to_vec();
    let list = org.shape(&Scene::layout(640.0, 1.0));
    for p in &list.prims {
        if let Prim::Curve { pts, .. } = p {
            let pts = list.pts(pts);
            let (a, b) = (pts[0], pts[pts.len() - 1]);
            let len = (b[0] - a[0]).hypot(b[1] - a[1]);
            for q in pts {
                let cross = ((b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0])).abs()
                    / len.max(1e-6);
                assert!(cross < 1e-2, "pseudopod bends by {cross}");
            }
        }
    }
    run(&mut org, &cs, BodyState::Thinking, true, 1.3);
    assert_eq!(org.organelles(), org_before.as_slice());
}

#[test]
fn dashes_satellites_and_status_pulses_hold_still() {
    let cs = cells();
    let mut org = Organism::new(Config::default());
    run(&mut org, &cs, BodyState::Thinking, true, 2.0);
    let scene = Scene::layout(640.0, 1.0);
    let snap = |o: &Organism| {
        let l = o.shape(&scene);
        // Keep everything except the (static) body-independent time terms: dash phases,
        // satellite circles and nucleus / error-cell radii.
        l.prims
            .iter()
            .filter_map(|p| match p {
                Prim::Circle { c, r, stroke, .. } => {
                    Some((c[0], c[1], *r, stroke.and_then(|s| s.dash.map(|d| d.phase))))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let a = snap(&org);
    run(&mut org, &cs, BodyState::Thinking, true, 0.9);
    let b = snap(&org);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert!((x.2 - y.2).abs() < 1e-4, "radius changes: {x:?} vs {y:?}");
        assert_eq!(x.3, y.3, "dash phase moves");
    }
    assert!(
        a.iter().any(|c| c.3 == Some(0.0)),
        "orbit is dashed while thinking"
    );
}

#[test]
fn waves_fade_without_expanding() {
    let mut org = Organism::new(Config::default());
    run(&mut org, &[], BodyState::Idle, true, 0.2);
    run(&mut org, &[], BodyState::Speaking, true, 0.5);
    assert!(!org.waves().is_empty());
    assert!(org.waves().iter().all(|w| w.r == 0.0));
    assert!(org.waves().iter().any(|w| w.life < 1.0));
}

#[test]
fn springs_are_monotone_and_do_not_overshoot() {
    let mut cs = cells();
    cs.push(CellInput::new("born", Engine::Pando, Status::Working).with_new(true));
    let mut org = Organism::new(Config::default());
    let mut inputs = Inputs::new(BodyState::Idle, &cs);
    inputs.reduced_motion = true;
    org.step(DT, &inputs);
    let target = |o: &Organism| {
        let c = o.cells().last().unwrap();
        let rc = 640.0 * o.params().ring_radius;
        (320.0 + c.ang.cos() * rc, 320.0 + c.ang.sin() * rc)
    };
    let dist = |o: &Organism| {
        let c = o.cells().last().unwrap();
        let t = target(o);
        (c.spring.x - t.0).hypot(c.spring.y - t.1)
    };
    let mut last = dist(&org);
    assert!(last > 100.0);
    for _ in 0..600 {
        org.step(DT, &inputs);
        let d = dist(&org);
        assert!(d <= last + 1e-9, "distance grew: {last} -> {d}");
        last = d;
    }
    assert!(last < 0.5);
    // The reference spring does overshoot from the same start (that is what we remove).
    let mut live = Organism::new(Config::default());
    let mut li = Inputs::new(BodyState::Idle, &cs);
    li.reduced_motion = false;
    live.step(DT, &li);
    let mut worst = 0.0_f64;
    let mut prev = dist(&live);
    for _ in 0..600 {
        live.step(DT, &li);
        let d = dist(&live);
        worst = worst.max(d - prev);
        prev = d;
    }
    assert!(worst > 1e-3, "non-reduced motion should wobble");
}

#[test]
fn small_canvas_drops_labels_and_keeps_strokes_legible() {
    let cs = cells();
    let mut org = Organism::new(Config::default());
    run(&mut org, &cs, BodyState::Idle, false, 3.0);
    let scene = Scene::layout(118.0, 1.0);
    let list: DrawList = org.shape(&scene);

    assert!(
        !list.prims.iter().any(|p| matches!(p, Prim::Text { .. })),
        "labels and badges hidden under 200 px"
    );
    // Body polygon: LOD 96 samples (+1 closing point).
    let body = list
        .prims
        .iter()
        .find_map(|p| match p {
            Prim::Poly {
                pts,
                strokes: [Some(h), _],
                ..
            } if h.width >= scene.min_halo => Some(pts.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(body.end - body.start, 97);

    for p in &list.prims {
        let widths: Vec<f32> = match p {
            Prim::Circle { stroke, .. } => stroke.iter().map(|s| s.width).collect(),
            Prim::Poly { strokes, .. } => strokes.iter().flatten().map(|s| s.width).collect(),
            Prim::Curve { stroke, .. } => vec![stroke.width],
            _ => vec![],
        };
        assert!(
            widths.iter().all(|w| *w >= 0.75),
            "stroke below the device floor: {widths:?}"
        );
    }
    // Halos keep at least 2 px.
    assert!(
        list.prims
            .iter()
            .any(|p| matches!(p, Prim::Curve { stroke, .. } if stroke.width >= 2.0))
    );

    // Everything lies inside the canvas and cells are still distinct blobs (>= 2 px radius,
    // centres further apart than their diameters).
    for q in &list.points {
        assert!(
            q[0] > -2.0 && q[0] < 120.0 && q[1] > -2.0 && q[1] < 120.0,
            "{q:?}"
        );
    }
    let mut centres = Vec::new();
    for c in org.cells() {
        let r = org.cell_radius(c) as f32 * scene.scale;
        assert!(r >= 2.0, "cell radius {r} px");
        centres.push((
            c.spring.x as f32 * scene.scale,
            c.spring.y as f32 * scene.scale,
            r,
        ));
    }
    for (i, a) in centres.iter().enumerate() {
        for b in &centres[i + 1..] {
            assert!(
                (a.0 - b.0).hypot(a.1 - b.1) > 2.0 * (a.2 + b.2),
                "cells overlap at 118 px"
            );
        }
    }
    // Organelle LOD.
    let discs: usize = list
        .prims
        .iter()
        .map(|p| match p {
            Prim::Discs { discs, .. } => (discs.end - discs.start) as usize,
            _ => 0,
        })
        .sum();
    assert_eq!(discs, 30);
}

#[test]
fn pointer_hit_radius_has_a_floor_on_small_canvases() {
    let cs = cells();
    let mut org = Organism::new(Config::default());
    run(&mut org, &cs, BodyState::Idle, false, 3.0);
    let c = org.cells()[0].spring;
    // 15 reference units away is outside 34*0.184 = 6 px but the floor (12 px) keeps it hovered.
    let mut inputs = Inputs::new(BodyState::Idle, &cs);
    inputs.canvas_px = 118.0;
    inputs.pointer = Some([(c.x + 45.0) / 640.0, c.y / 640.0]);
    for _ in 0..60 {
        org.step(DT, &inputs);
    }
    assert!(org.cells()[0].hover > 0.9, "hover {}", org.cells()[0].hover);
    inputs.canvas_px = 640.0;
    let mut org2 = Organism::new(Config::default());
    run(&mut org2, &cs, BodyState::Idle, false, 3.0);
    for _ in 0..60 {
        org2.step(DT, &inputs);
    }
    assert!(org2.cells()[0].hover < 0.1);
}
