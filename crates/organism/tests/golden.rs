//! Parity of the Rust simulation with `organism.html`, replayed from the JS goldens.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::{golden, replay};
use organism::{Config, Organism};
use serde_json::Value;

const STATE_TOL: f64 = 1e-8;

fn close(name: &str, frame: u64, got: f64, want: f64) {
    assert!(
        (got - want).abs() <= STATE_TOL,
        "frame {frame}: {name}: rust {got} vs js {want} (diff {})",
        (got - want).abs()
    );
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn compare_state(frame: u64, org: &Organism, want: &Value) {
    close("t", frame, org.time(), f(&want["t"]));
    close("amp", frame, org.amplitude(), f(&want["amp"]));
    close("ampT", frame, org.amplitude_clock(), f(&want["ampT"]));
    close("nx", frame, org.nucleus()[0], f(&want["nx"]));
    close("ny", frame, org.nucleus()[1], f(&want["ny"]));

    let waves = want["waves"].as_array().unwrap();
    assert_eq!(org.waves().len(), waves.len(), "frame {frame}: wave count");
    for (w, j) in org.waves().iter().zip(waves) {
        close("wave.r", frame, w.r, f(&j["r"]));
        close("wave.life", frame, w.life, f(&j["life"]));
    }

    let parts = want["particles"].as_array().unwrap();
    assert_eq!(org.organelles().len(), parts.len());
    for (p, j) in org.organelles().iter().zip(parts) {
        close("p.a", frame, p.a, f(&j["a"]));
        close("p.r", frame, p.r, f(&j["r"]));
        close("p.s", frame, p.s, f(&j["s"]));
        close("p.sz", frame, p.sz, f(&j["sz"]));
    }

    let cells = want["cells"].as_array().unwrap();
    assert_eq!(org.cells().len(), cells.len(), "frame {frame}: cell count");
    for (c, j) in org.cells().iter().zip(cells) {
        assert_eq!(
            c.id().as_str(),
            j["id"].as_str().unwrap(),
            "frame {frame}: cell order"
        );
        close("x", frame, c.spring.x, f(&j["x"]));
        close("y", frame, c.spring.y, f(&j["y"]));
        close("vx", frame, c.spring.vx, f(&j["vx"]));
        close("vy", frame, c.spring.vy, f(&j["vy"]));
        close("born", frame, c.born, f(&j["born"]));
        close("phase", frame, c.phase, f(&j["phase"]));
        close("hover", frame, c.hover, f(&j["hover"]));
        close("ang", frame, c.ang, f(&j["ang"]));
        let flow = j["flow"].as_array().unwrap();
        assert_eq!(c.flow.len(), flow.len());
        for (a, b) in c.flow.iter().zip(flow) {
            close("flow", frame, *a, f(b));
        }
    }
}

#[test]
fn simulation_state_matches_reference_at_recorded_frames() {
    let g = golden();
    let frames = g["frames"].as_array().unwrap();
    let mut checked = 0;
    replay(
        Config {
            parity: true,
            seed: g["seed"].as_u64().unwrap() as u32,
        },
        |frame, org| {
            if let Some(rec) = frames.iter().find(|r| r["f"].as_u64() == Some(frame)) {
                compare_state(frame, org, &rec["state"]);
                checked += 1;
            }
        },
    );
    assert_eq!(checked, frames.len());
}

// ---------------------------------------------------------------------------------------
// Draw-list parity
// ---------------------------------------------------------------------------------------

use organism::Scene;
use organism::draw::{DrawList, Prim, Rgba, Stroke};

const PT_TOL: f64 = 1e-3;
const ALPHA_TOL: f64 = 1e-4;

#[derive(Debug)]
enum Geo {
    Circle([f64; 2], f64),
    Poly(Vec<[f64; 2]>, bool),
    Curve(Vec<[f64; 2]>),
    None,
}

#[derive(Debug)]
struct Ev {
    kind: &'static str,
    geo: Geo,
    color: Rgba,
    width: f64,
    dash: Option<[f64; 3]>,
    clip: bool,
    text: Option<(String, [f64; 2], f64, u16)>,
}

fn pts_of(list: &DrawList, r: &std::ops::Range<u32>) -> Vec<[f64; 2]> {
    list.pts(r)
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect()
}

fn stroke_ev(geo: Geo, s: &Stroke, clip: bool) -> Ev {
    Ev {
        kind: "stroke",
        geo,
        color: s.color,
        width: f64::from(s.width),
        dash: s
            .dash
            .map(|d| [f64::from(d.on), f64::from(d.off), f64::from(d.phase)]),
        clip,
        text: None,
    }
}

fn fill_ev(geo: Geo, c: Rgba, clip: bool) -> Ev {
    Ev {
        kind: "fill",
        geo,
        color: c,
        width: 0.0,
        dash: None,
        clip,
        text: None,
    }
}

fn expand(list: &DrawList) -> Vec<Ev> {
    let mut out = Vec::new();
    let mut clip = false;
    for p in &list.prims {
        match p {
            Prim::Circle { c, r, fill, stroke } => {
                let g = || Geo::Circle([f64::from(c[0]), f64::from(c[1])], f64::from(*r));
                if let Some(f) = fill {
                    out.push(fill_ev(g(), *f, clip));
                }
                if let Some(s) = stroke {
                    out.push(stroke_ev(g(), s, clip));
                }
            }
            Prim::Poly {
                pts,
                closed,
                fill,
                strokes,
            } => {
                let g = || Geo::Poly(pts_of(list, pts), *closed);
                if let Some(f) = fill {
                    out.push(fill_ev(g(), *f, clip));
                }
                for s in strokes.iter().flatten() {
                    out.push(stroke_ev(g(), s, clip));
                }
            }
            Prim::Curve { pts, stroke } => {
                out.push(stroke_ev(Geo::Curve(pts_of(list, pts)), stroke, clip))
            }
            Prim::Text {
                text,
                pos,
                size,
                weight,
                color,
            } => out.push(Ev {
                kind: "text",
                geo: Geo::None,
                color: *color,
                width: 0.0,
                dash: None,
                clip,
                text: Some((
                    text.as_str().to_owned(),
                    [f64::from(pos[0]), f64::from(pos[1])],
                    f64::from(*size),
                    *weight,
                )),
            }),
            Prim::ClipBegin { .. } => clip = true,
            Prim::ClipEnd => clip = false,
            Prim::Discs { .. } => panic!("parity scenes must not batch discs"),
        }
    }
    out
}

fn pt_close(frame: u64, i: usize, what: &str, a: &[f64], b: &[f64]) {
    for (x, y) in a.iter().zip(b) {
        assert!(
            (x - y).abs() <= PT_TOL,
            "frame {frame} op {i}: {what}: rust {a:?} vs js {b:?}"
        );
    }
}

fn arr2(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(f).collect()
}

fn compare_ops(frame: u64, ev: &[Ev], ops: &[Value]) {
    for (i, (e, j)) in ev.iter().zip(ops).enumerate() {
        let jk = j["k"].as_str().unwrap();
        assert_eq!(
            e.kind, jk,
            "frame {frame} op {i}: kind (rust {e:?} vs js {j})"
        );
        assert_eq!(
            e.clip,
            j["clip"].as_bool().unwrap_or(false),
            "frame {frame} op {i}: clip"
        );
        let jc = arr2(&j["color"]);
        let rgb = e.color.rgb8();
        for k in 0..3 {
            assert_eq!(
                f64::from(rgb[k]),
                jc[k],
                "frame {frame} op {i}: colour channel {k} ({e:?} vs {j})"
            );
        }
        assert!(
            (f64::from(e.color.a) - jc[3]).abs() <= ALPHA_TOL,
            "frame {frame} op {i}: alpha {} vs {} ({e:?} vs {j})",
            e.color.a,
            jc[3]
        );
        if jk == "text" {
            let (text, pos, size, weight) = e.text.as_ref().unwrap();
            assert_eq!(text, j["text"].as_str().unwrap());
            pt_close(frame, i, "text pos", pos, &[f(&j["x"]), f(&j["y"])]);
            assert_eq!(*size, f(&j["size"]));
            assert_eq!(f64::from(*weight), f(&j["weight"]));
            continue;
        }
        let g = &j["g"];
        match (&e.geo, g["t"].as_str().unwrap()) {
            (Geo::Circle(c, r), "circle") => {
                pt_close(frame, i, "circle c", c, &arr2(&g["c"]));
                pt_close(frame, i, "circle r", &[*r], &[f(&g["r"])]);
            }
            (Geo::Poly(p, closed), "poly") => {
                assert_eq!(
                    *closed,
                    g["closed"].as_bool().unwrap(),
                    "frame {frame} op {i}: closed"
                );
                let jp = g["pts"].as_array().unwrap();
                assert_eq!(p.len(), jp.len(), "frame {frame} op {i}: poly length");
                for (a, b) in p.iter().zip(jp) {
                    pt_close(frame, i, "poly pt", a, &arr2(b));
                }
            }
            (Geo::Curve(p), "curve") => {
                let jp = g["pts"].as_array().unwrap();
                assert_eq!(p.len(), jp.len(), "frame {frame} op {i}: curve length");
                for (a, b) in p.iter().zip(jp) {
                    pt_close(frame, i, "curve pt", a, &arr2(b));
                }
            }
            (other, t) => panic!("frame {frame} op {i}: geometry {other:?} vs js {t}"),
        }
        if jk == "stroke" {
            assert!(
                (e.width - f(&j["w"])).abs() <= 1e-4,
                "frame {frame} op {i}: width {} vs {}",
                e.width,
                j["w"]
            );
            let jd = arr2(&j["dash"]);
            match e.dash {
                None => assert!(
                    jd.is_empty(),
                    "frame {frame} op {i}: js dashed {jd:?}, rust solid"
                ),
                Some(d) => {
                    assert_eq!(jd.len(), 2, "frame {frame} op {i}: rust dashed, js {jd:?}");
                    pt_close(frame, i, "dash", &d[..2], &jd);
                    // Only the nucleus orbit carries a deliberate phase (lineDashOffset = -20 t).
                    if matches!(e.geo, Geo::Circle(..)) {
                        pt_close(frame, i, "dash phase", &d[2..], &[f(&j["off"])]);
                    }
                }
            }
        }
    }
    assert_eq!(ev.len(), ops.len(), "frame {frame}: number of draw ops");
}

#[test]
fn draw_list_matches_reference_draw_calls() {
    let g = golden();
    let frames = g["frames"].as_array().unwrap();
    let scene = Scene::reference();
    let mut counts = Vec::new();
    replay(
        Config {
            parity: true,
            seed: g["seed"].as_u64().unwrap() as u32,
        },
        |frame, org| {
            if let Some(rec) = frames.iter().find(|r| r["f"].as_u64() == Some(frame)) {
                let list = org.shape(&scene);
                let ev = expand(&list);
                compare_ops(frame, &ev, rec["ops"].as_array().unwrap());
                counts.push((frame, list.prims.len()));
            }
        },
    );
    assert_eq!(counts.len(), frames.len());
}

#[test]
fn primitive_count_is_stable_while_the_cell_set_is_stable() {
    // Frames 0..200 have the same 7 cells in the same statuses: the count may only change
    // with the thinking state (3 satellites, no new prims otherwise).
    let g = golden();
    let scene = Scene::reference();
    let mut by_state: std::collections::HashMap<(usize, bool), std::collections::HashSet<usize>> =
        Default::default();
    replay(
        Config {
            parity: true,
            seed: g["seed"].as_u64().unwrap() as u32,
        },
        |frame, org| {
            if frame < 195 && frame % 3 == 0 {
                let thinking = org.state() == organism::BodyState::Thinking;
                by_state
                    .entry((org.cells().len(), thinking))
                    .or_default()
                    .insert(org.shape(&scene).prims.len());
            }
        },
    );
    for (k, v) in by_state {
        assert!(v.len() <= 2, "{k:?}: {v:?}");
    }
}
