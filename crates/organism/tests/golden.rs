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
