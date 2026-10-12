//! Visual verification of the raster backend against the reference page in Chrome
//! (PDA-T-0041). `crates/organism/harness/chrome_capture.mjs` captures the real
//! `organism.html` replaying the golden timeline; the ignored test renders the same frames
//! with tiny-skia (parity draw list) and compares them with SSIM.
//!
//! Run: `script/organism-visual.sh` (captures with Chrome, then runs the test).

use organism::{
    BodyState, CellId, CellInput, Config, DrawList, Engine, Inputs, Organism, Scene, Status,
};
use serde_json::Value;

const GOLDEN: &str = include_str!("../../../organism/tests/golden/organism.json");

fn parse_state(s: &str) -> BodyState {
    match s {
        "listening" => BodyState::Listening,
        "thinking" => BodyState::Thinking,
        "speaking" => BodyState::Speaking,
        "delegating" => BodyState::Delegating,
        _ => BodyState::Idle,
    }
}

fn parse_engine(s: &str) -> Engine {
    match s {
        "acp" => Engine::ClaudeAcp,
        "external" => Engine::ClaudeExternal,
        "cli" => Engine::Cli,
        _ => Engine::Pando,
    }
}

fn parse_status(s: &str) -> Status {
    match s {
        "pending" => Status::Pending,
        "waiting" => Status::Waiting,
        "paused" => Status::Paused,
        "done" => Status::Done,
        "error" => Status::Error,
        _ => Status::Working,
    }
}

fn agent(v: &Value) -> CellInput {
    CellInput::new(
        v["id"].as_str().unwrap_or("?"),
        parse_engine(v["engine"].as_str().unwrap_or("pando")),
        parse_status(v["status"].as_str().unwrap_or("working")),
    )
    .with_new(v["isNew"].as_bool().unwrap_or(false))
}

/// The parity draw list of the golden timeline at `frame` (reference scene, 640 px).
#[must_use]
pub fn golden_draw_list(frame: u64) -> Option<DrawList> {
    let g: Value = serde_json::from_str(GOLDEN).ok()?;
    let dt = 1.0 / g["fps"].as_f64()?;
    let mut agents: Vec<CellInput> = g["initialAgents"].as_array()?.iter().map(agent).collect();
    let mut state = BodyState::Idle;
    let mut pointer: Option<[f64; 2]> = None;
    let mut selected: Option<CellId> = g["initialSelected"].as_str().map(CellId::new);
    let timeline = g["timeline"].as_array()?;
    let mut org = Organism::new(Config {
        parity: true,
        seed: g["seed"].as_u64()? as u32,
    });
    for f in 0..=frame {
        for e in timeline.iter().filter(|e| e["f"].as_u64() == Some(f)) {
            match e["op"].as_str()? {
                "state" => state = parse_state(e["value"].as_str()?),
                "pointer" => {
                    pointer = e["value"].as_array().map(|p| {
                        [
                            p[0].as_f64().unwrap_or(0.0) / 640.0,
                            p[1].as_f64().unwrap_or(0.0) / 640.0,
                        ]
                    });
                }
                "spawn" => agents.push(agent(&e["agent"])),
                "kill" => {
                    agents.pop();
                }
                "select" => selected = e["id"].as_str().map(CellId::new),
                _ => {}
            }
        }
        let mut inputs = Inputs::new(state, &agents);
        inputs.pointer = pointer;
        inputs.selected.clone_from(&selected);
        org.step(dt, &inputs);
    }
    Some(org.shape(&Scene::reference()))
}

/// Mean structural similarity of two equally sized luma planes (8x8 windows, stride 4,
/// the usual `K1 = 0.01`, `K2 = 0.03`, `L = 255`).
#[must_use]
pub fn ssim(a: &[f32], b: &[f32], w: usize, h: usize) -> f64 {
    const WIN: usize = 8;
    const STEP: usize = 4;
    let c1 = (0.01_f64 * 255.0).powi(2);
    let c2 = (0.03_f64 * 255.0).powi(2);
    let (mut sum, mut n) = (0.0, 0usize);
    let mut y = 0;
    while y + WIN <= h {
        let mut x = 0;
        while x + WIN <= w {
            let (mut ma, mut mb) = (0.0_f64, 0.0_f64);
            for j in 0..WIN {
                for i in 0..WIN {
                    ma += f64::from(a[(y + j) * w + x + i]);
                    mb += f64::from(b[(y + j) * w + x + i]);
                }
            }
            let k = (WIN * WIN) as f64;
            let (ma, mb) = (ma / k, mb / k);
            let (mut va, mut vb, mut cov) = (0.0, 0.0, 0.0);
            for j in 0..WIN {
                for i in 0..WIN {
                    let da = f64::from(a[(y + j) * w + x + i]) - ma;
                    let db = f64::from(b[(y + j) * w + x + i]) - mb;
                    va += da * da;
                    vb += db * db;
                    cov += da * db;
                }
            }
            let (va, vb, cov) = (va / (k - 1.0), vb / (k - 1.0), cov / (k - 1.0));
            sum += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            n += 1;
            x += STEP;
        }
        y += STEP;
    }
    if n == 0 { 1.0 } else { sum / n as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organism_view::raster;
    use organism::draw::Rgba;

    fn luma(rgba: &[u8]) -> Vec<f32> {
        rgba.as_chunks::<4>()
            .0
            .iter()
            .map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
            .collect()
    }

    #[test]
    fn ssim_of_identical_planes_is_one_and_noise_lowers_it() {
        let a: Vec<f32> = (0..64 * 64).map(|i| ((i * 7) % 255) as f32).collect();
        assert!((ssim(&a, &a, 64, 64) - 1.0).abs() < 1e-9);
        let b: Vec<f32> = a
            .iter()
            .enumerate()
            .map(|(i, v)| (v + (i % 13) as f32 * 9.0) % 255.0)
            .collect();
        assert!(ssim(&a, &b, 64, 64) < 0.9);
    }

    #[test]
    fn golden_replay_builds_a_parity_draw_list() {
        let list = golden_draw_list(120).expect("replay");
        assert!(list.prims.len() > 50);
    }

    /// Needs `PANDO_CHROME_CAPTURE_DIR` with `chrome-<frame>.png` files from `chrome_capture.mjs`.
    #[test]
    #[ignore = "needs Chrome captures: script/organism-visual.sh"]
    fn raster_matches_chrome_capture() {
        let dir = std::env::var("PANDO_CHROME_CAPTURE_DIR").expect("PANDO_CHROME_CAPTURE_DIR");
        let bg = Rgba::from_token(organism::tokens::BOSQUE);
        let mut worst: f64 = 1.0;
        for frame in [30_u64, 120, 210, 300, 450, 600] {
            let path = std::path::Path::new(&dir).join(format!("chrome-{frame}.png"));
            let Ok(img) = image::open(&path) else {
                continue;
            };
            let chrome = img.to_rgba8();
            assert_eq!((chrome.width(), chrome.height()), (640, 640), "{path:?}");
            let list = golden_draw_list(frame).expect("replay");
            let pm = raster::render(&list, 640.0, 640, Some(bg)).expect("pixmap");
            let ours = luma(pm.data());
            let s = ssim(&luma(chrome.as_raw()), &ours, 640, 640);
            println!("frame {frame}: SSIM {s:.4}");
            pm.save_png(std::path::Path::new(&dir).join(format!("rust-{frame}.png")))
                .expect("png");
            worst = worst.min(s);
        }
        assert!(worst >= 0.97, "worst SSIM {worst:.4}");
    }
}
