//! Shared helpers: replay the golden timeline recorded by `harness/harness.mjs`.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use organism::{BodyState, CellId, CellInput, Config, Engine, Inputs, Organism, Status};
use serde_json::Value;

pub const GOLDEN: &str = include_str!("../golden/organism.json");

pub fn golden() -> Value {
    serde_json::from_str(GOLDEN).expect("parse golden")
}

pub fn parse_state(s: &str) -> BodyState {
    match s {
        "idle" => BodyState::Idle,
        "listening" => BodyState::Listening,
        "thinking" => BodyState::Thinking,
        "speaking" => BodyState::Speaking,
        "delegating" => BodyState::Delegating,
        other => panic!("unknown state {other}"),
    }
}

pub fn parse_engine(s: &str) -> Engine {
    match s {
        "pando" => Engine::Pando,
        "acp" => Engine::ClaudeAcp,
        "external" => Engine::ClaudeExternal,
        "cli" => Engine::Cli,
        other => panic!("unknown engine {other}"),
    }
}

pub fn parse_status(s: &str) -> Status {
    match s {
        "working" => Status::Working,
        "waiting" => Status::Waiting,
        "paused" => Status::Paused,
        "done" => Status::Done,
        "error" => Status::Error,
        other => panic!("unknown status {other}"),
    }
}

pub fn agent(v: &Value) -> CellInput {
    let id = v["id"].as_str().expect("id");
    CellInput::new(
        id,
        parse_engine(v["engine"].as_str().expect("engine")),
        parse_status(v["status"].as_str().expect("status")),
    )
    .with_new(v["isNew"].as_bool().unwrap_or(false))
}

/// Replays the recorded timeline; `on_frame(frame, &Organism)` is called after each step.
pub fn replay(cfg: Config, mut on_frame: impl FnMut(u64, &Organism)) {
    let g = golden();
    let dt = 1.0 / g["fps"].as_f64().expect("fps");
    let last = g["frames_recorded"]
        .as_array()
        .expect("frames")
        .iter()
        .map(|v| v.as_u64().unwrap())
        .max()
        .unwrap();
    let mut agents: Vec<CellInput> = g["initialAgents"]
        .as_array()
        .expect("agents")
        .iter()
        .map(agent)
        .collect();
    let mut state = BodyState::Idle;
    let mut pointer: Option<[f64; 2]> = None;
    let mut selected: Option<CellId> = g["initialSelected"].as_str().map(CellId::new);
    let timeline = g["timeline"].as_array().expect("timeline");
    let mut org = Organism::new(cfg);
    for f in 0..=last {
        for e in timeline.iter().filter(|e| e["f"].as_u64() == Some(f)) {
            match e["op"].as_str().expect("op") {
                "state" => state = parse_state(e["value"].as_str().expect("state")),
                "pointer" => {
                    pointer = e["value"].as_array().map(|p| {
                        [
                            p[0].as_f64().unwrap() / 640.0,
                            p[1].as_f64().unwrap() / 640.0,
                        ]
                    });
                }
                "spawn" => agents.push(agent(&e["agent"])),
                "kill" => {
                    agents.pop();
                }
                "select" => selected = e["id"].as_str().map(CellId::new),
                other => panic!("unknown op {other}"),
            }
        }
        let mut inputs = Inputs::new(state, &agents);
        inputs.pointer = pointer;
        inputs.selected.clone_from(&selected);
        org.step(dt, &inputs);
        on_frame(f, &org);
    }
}
