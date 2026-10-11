//! `step + shape` cost for 6 and 32 cells (budget: < 0.3 ms / < 1 ms on a reference x86).

use criterion::{Criterion, criterion_group, criterion_main};
use organism::{BodyState, CellInput, Config, DrawList, Engine, Inputs, Organism, Scene, Status};
use std::hint::black_box;

fn cells(n: usize) -> Vec<CellInput> {
    let engines = [
        Engine::Pando,
        Engine::ClaudeAcp,
        Engine::ClaudeExternal,
        Engine::Cli,
    ];
    let statuses = [
        Status::Working,
        Status::Done,
        Status::Waiting,
        Status::Working,
        Status::Paused,
        Status::Error,
    ];
    (0..n)
        .map(|i| CellInput::new(&format!("cell-{i}"), engines[i % 4], statuses[i % 6]))
        .collect()
}

fn bench(c: &mut Criterion) {
    for n in [6usize, 32] {
        let cs = cells(n);
        let mut org = Organism::new(Config::default());
        let scene = Scene::layout(640.0, 1.0);
        let mut list = DrawList::new();
        let mut inputs = Inputs::new(BodyState::Thinking, &cs);
        inputs.pointer = Some([0.6, 0.4]);
        for _ in 0..300 {
            org.step(1.0 / 60.0, &inputs);
        }
        c.bench_function(&format!("step_shape_{n}_cells"), |b| {
            b.iter(|| {
                org.step(1.0 / 60.0, black_box(&inputs));
                org.shape_into(&scene, &mut list);
                black_box(list.prims.len())
            });
        });
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
