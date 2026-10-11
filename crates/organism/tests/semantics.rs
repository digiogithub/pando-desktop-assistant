//! Semantics must describe what is drawn.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use organism::draw::Prim;
use organism::{BodyState, CellInput, Config, Engine, Inputs, Organism, Scene, Status};

fn setup(n: usize) -> (Organism, Vec<CellInput>) {
    let engines = [
        Engine::Pando,
        Engine::ClaudeAcp,
        Engine::ClaudeExternal,
        Engine::Cli,
    ];
    let statuses = [
        Status::Working,
        Status::Waiting,
        Status::Done,
        Status::Error,
        Status::Paused,
    ];
    let cs: Vec<_> = (0..n)
        .map(|i| CellInput::new(&format!("cell-{i}"), engines[i % 4], statuses[i % 5]))
        .collect();
    let mut org = Organism::new(Config::default());
    let mut inputs = Inputs::new(BodyState::Idle, &cs);
    inputs.selected = Some("cell-1".into());
    for _ in 0..240 {
        org.step(1.0 / 60.0, &inputs);
    }
    (org, cs)
}

#[test]
fn semantics_match_drawn_positions_within_one_pixel() {
    for size in [640.0_f32, 300.0, 118.0] {
        let (org, _) = setup(6);
        let scene = Scene::layout(size, 1.0);
        let list = org.shape(&scene);
        let sem = org.semantics(&scene);
        assert_eq!(sem.len(), 6);
        for c in &sem {
            let [cx, cy] = c.bounds.centre();
            // Cell nucleus / membrane centre: the drawn label is centred on the cell when
            // labels are visible, otherwise compare with the cell's selection or nucleus ring.
            if scene.lod.labels {
                let label = list
                    .prims
                    .iter()
                    .find_map(|p| match p {
                        Prim::Text { text, pos, .. } if text.as_str() == &*c.label => Some(*pos),
                        _ => None,
                    })
                    .expect("label drawn");
                assert!(
                    (label[0] - cx).abs() <= 1.0,
                    "{size}: label x {} vs {cx}",
                    label[0]
                );
                assert!(label[1] > cy, "label sits below the cell");
            }
            // The nucleus circle is centred on the cell.
            let near = list.prims.iter().any(|p| match p {
                Prim::Circle { c: pc, .. } => {
                    (pc[0] - cx).abs() <= 1.0 && (pc[1] - cy).abs() <= 1.0
                }
                _ => false,
            });
            assert!(near, "{size}: no drawn circle centred on {cx},{cy}");
            // Bounds enclose the drawn membrane polygon of this cell.
            let enclosed = list.prims.iter().any(|p| match p {
                Prim::Poly {
                    pts,
                    closed: true,
                    fill: Some(f),
                    ..
                } if f.a < 0.1 => {
                    let ps = list.pts(pts);
                    let (sx, sy) = ps.iter().fold((0.0, 0.0), |a, q| (a.0 + q[0], a.1 + q[1]));
                    let (mx, my) = (sx / ps.len() as f32, sy / ps.len() as f32);
                    (mx - cx).abs() <= 1.0
                        && (my - cy).abs() <= 1.0
                        && ps.iter().all(|q| c.bounds.contains(*q))
                }
                _ => false,
            });
            assert!(
                enclosed,
                "{size}: bounds do not enclose the membrane of {}",
                c.id.as_str()
            );
        }
    }
}

#[test]
fn semantics_expose_ids_status_selection_and_ring_order() {
    let (org, _) = setup(5);
    let scene = Scene::layout(640.0, 1.0);
    let sem = org.semantics(&scene);
    let ids: Vec<_> = sem.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["cell-0", "cell-1", "cell-2", "cell-3", "cell-4"]);
    assert_eq!(sem[1].engine_id, "acp");
    assert_eq!(sem[1].status_id, "waiting");
    assert_eq!(sem[3].engine_label, "CLI · spore, output only");
    assert!(sem[1].selected && sem.iter().filter(|c| c.selected).count() == 1);
    assert!(sem.iter().all(|c| !c.aggregate && c.folded.is_empty()));
    assert!(sem.iter().all(|c| c.hit_radius >= 12.0));
}

#[test]
fn hit_test_picks_the_nearest_cell_inside_the_hit_radius() {
    let (org, _) = setup(6);
    let scene = Scene::layout(640.0, 1.0);
    for c in org.semantics(&scene) {
        assert_eq!(org.hit_test(&scene, c.bounds.centre()), Some(c.id.clone()));
    }
    assert_eq!(org.hit_test(&scene, [1.0, 1.0]), None);
    // On a small canvas the 12 px floor applies.
    let small = Scene::layout(118.0, 1.0);
    let c = org.semantics(&small).remove(0);
    let [cx, cy] = c.bounds.centre();
    assert_eq!(org.hit_test(&small, [cx + 11.0, cy]), Some(c.id));
}

#[test]
fn dying_cells_are_not_exposed_and_the_fold_is_described() {
    let mut org = Organism::new(Config::default());
    let mut cs: Vec<_> = (0..14)
        .map(|i| CellInput::new(&format!("d{i}"), Engine::Pando, Status::Done))
        .collect();
    cs.push(CellInput::new("live", Engine::Pando, Status::Working));
    let mut inputs = Inputs::new(BodyState::Idle, &cs);
    for _ in 0..120 {
        org.step(1.0 / 60.0, &inputs);
    }
    let scene = Scene::layout(640.0, 1.0);
    let sem = org.semantics(&scene);
    assert_eq!(sem.len(), 12);
    let agg = sem.iter().find(|c| c.aggregate).expect("aggregate");
    assert_eq!(&*agg.label, "+4");
    assert_eq!(agg.folded.len(), 4);
    // Drop everything: cells are dying and no longer exposed.
    inputs.cells = &[];
    org.step(1.0 / 60.0, &inputs);
    assert!(org.semantics(&scene).is_empty());
}
