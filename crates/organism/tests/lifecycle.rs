//! Lifecycle behaviour beyond the reference: stable slots, death, birth, Delegating, fold.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use organism::draw::Prim;
use organism::sim::Life;
use organism::{BodyState, CellInput, Config, DrawList, Engine, Inputs, Organism, Scene, Status};

const DT: f64 = 1.0 / 60.0;

fn cell(id: &str, status: Status) -> CellInput {
    CellInput::new(id, Engine::Pando, status)
}

fn run(org: &mut Organism, cells: &[CellInput], state: BodyState, secs: f64) {
    let inputs = Inputs::new(state, cells);
    for _ in 0..(secs / DT).round() as usize {
        org.step(DT, &inputs);
    }
}

fn ang_of(org: &Organism, id: &str) -> f64 {
    org.cells()
        .iter()
        .find(|c| c.id().as_str() == id)
        .unwrap()
        .ang
}

#[test]
fn adding_or_removing_a_cell_does_not_move_the_others_slots() {
    let mut org = Organism::new(Config::default());
    let mut cells: Vec<_> = (0..4)
        .map(|i| cell(&format!("c{i}"), Status::Working))
        .collect();
    run(&mut org, &cells, BodyState::Idle, 1.0);
    let before: Vec<f64> = (0..4).map(|i| ang_of(&org, &format!("c{i}"))).collect();

    cells.push(cell("c4", Status::Working));
    run(&mut org, &cells, BodyState::Idle, 0.1);
    for (i, b) in before.iter().enumerate() {
        assert_eq!(
            ang_of(&org, &format!("c{i}")),
            *b,
            "slot jump on add (c{i})"
        );
    }

    cells.remove(1); // c1 leaves
    run(&mut org, &cells, BodyState::Idle, 0.1);
    for i in [0, 2, 3, 4] {
        let want = if i == 4 {
            ang_of(&org, "c4")
        } else {
            before[i]
        };
        assert_eq!(
            ang_of(&org, &format!("c{i}")),
            want,
            "slot jump on remove (c{i})"
        );
    }
}

#[test]
fn freed_slot_is_reused_by_the_next_cell() {
    let mut org = Organism::new(Config::default());
    let mut cells: Vec<_> = (0..4)
        .map(|i| cell(&format!("c{i}"), Status::Working))
        .collect();
    run(&mut org, &cells, BodyState::Idle, 0.5);
    let gap = org.cells()[1].slot;
    cells.remove(1);
    run(&mut org, &cells, BodyState::Idle, 1.0); // death animation over
    assert!(org.cells().iter().all(|c| c.id().as_str() != "c1"));
    cells.push(cell("late", Status::Working));
    run(&mut org, &cells, BodyState::Idle, 0.1);
    assert_eq!(
        org.cells()
            .iter()
            .find(|c| c.id().as_str() == "late")
            .unwrap()
            .slot,
        gap
    );
}

#[test]
fn dying_cell_fades_retracts_and_is_removed_after_the_death_duration() {
    let mut org = Organism::new(Config::default());
    let cells = vec![cell("a", Status::Working), cell("b", Status::Working)];
    run(&mut org, &cells, BodyState::Idle, 1.0);
    let only_a = vec![cell("a", Status::Working)];
    run(&mut org, &only_a, BodyState::Idle, 0.3);
    let b = org
        .cells()
        .iter()
        .find(|c| c.id().as_str() == "b")
        .expect("still dying");
    assert!(matches!(b.life, Life::Dying { .. }));
    assert!((b.fade - 0.5).abs() < 0.05, "fade {}", b.fade);
    // Draw list still has it, with reduced alpha; pseudopod is shorter.
    let list = org.shape(&Scene::layout(640.0, 1.0));
    assert!(list.prims.iter().any(
        |p| matches!(p, Prim::Text { text, color, .. } if text.as_str() == "b" && color.a < 0.5)
    ));
    run(&mut org, &only_a, BodyState::Idle, 0.35);
    assert!(org.cells().iter().all(|c| c.id().as_str() != "b"));
}

#[test]
fn dying_cell_revives_when_it_reappears() {
    let mut org = Organism::new(Config::default());
    let cells = vec![cell("a", Status::Working), cell("b", Status::Working)];
    run(&mut org, &cells, BodyState::Idle, 1.0);
    let slot = org.cells()[1].slot;
    run(&mut org, &cells[..1], BodyState::Idle, 0.3);
    run(&mut org, &cells, BodyState::Idle, 0.5);
    let b = org.cells().iter().find(|c| c.id().as_str() == "b").unwrap();
    assert!(matches!(b.life, Life::Alive));
    assert_eq!(b.slot, slot);
    assert!(b.fade > 0.95);
}

#[test]
fn removing_all_cells_leaves_an_empty_ring() {
    let mut org = Organism::new(Config::default());
    let cells: Vec<_> = (0..5)
        .map(|i| cell(&format!("c{i}"), Status::Done))
        .collect();
    run(&mut org, &cells, BodyState::Idle, 1.0);
    run(&mut org, &[], BodyState::Idle, 1.0);
    assert!(org.cells().is_empty());
}

#[test]
fn gaps_are_compacted_only_after_the_quiet_period() {
    let mut org = Organism::new(Config::default());
    let mut cells: Vec<_> = (0..5)
        .map(|i| cell(&format!("c{i}"), Status::Working))
        .collect();
    run(&mut org, &cells, BodyState::Idle, 1.0);
    cells.remove(1);
    run(&mut org, &cells, BodyState::Idle, 5.0);
    assert_eq!(
        org.cells().iter().map(|c| c.slot).max().unwrap(),
        4,
        "gap kept at first"
    );
    run(&mut org, &cells, BodyState::Idle, 6.0);
    let mut slots: Vec<_> = org.cells().iter().map(|c| c.slot).collect();
    slots.sort_unstable();
    assert_eq!(slots, vec![0, 1, 2, 3], "compacted in creation order");
    assert_eq!(ang_of(&org, "c2"), org.slot_angle(1));
}

#[test]
fn birth_emits_its_own_wave_and_starts_at_the_centre() {
    let mut org = Organism::new(Config::default());
    let mut cells = vec![cell("a", Status::Working)];
    run(&mut org, &cells, BodyState::Idle, 2.0);
    assert!(org.waves().is_empty());
    cells.push(cell("new", Status::Working).with_new(true));
    run(&mut org, &cells, BodyState::Idle, 1.0 / 60.0);
    assert_eq!(org.waves().len(), 1);
    let c = org
        .cells()
        .iter()
        .find(|c| c.id().as_str() == "new")
        .unwrap();
    assert!((c.spring.x - 320.0).abs() < 5.0 && (c.spring.y - 320.0).abs() < 5.0);
    // A cell that merely appears (startup, reconnect) does not make a wave.
    let mut quiet = Organism::new(Config::default());
    run(&mut quiet, &[cell("x", Status::Done)], BodyState::Idle, 0.5);
    assert!(quiet.waves().is_empty());
}

#[test]
fn delegating_overlay_lasts_one_and_a_half_seconds_and_returns_to_the_previous_state() {
    let mut org = Organism::new(Config::default());
    let cells = vec![cell("a", Status::Working)];
    run(&mut org, &cells, BodyState::Thinking, 1.0);
    let mut control = org.clone();
    org.delegate();
    assert_eq!(org.waves().len(), 1);
    run(&mut org, &cells, BodyState::Thinking, 0.1);
    assert_eq!(org.shown_state(), BodyState::Delegating);
    assert_eq!(org.state(), BodyState::Thinking, "base state untouched");
    run(&mut org, &cells, BodyState::Thinking, 0.65);
    // Mid-overlay the body carries the extra bulge, compared with an organism that never
    // delegated.
    run(&mut control, &cells, BodyState::Thinking, 0.75);
    let (with, without) = (org.body_radius(0.3), control.body_radius(0.3));
    assert!(with > without + 1.0, "{with} vs {without}");
    run(&mut org, &cells, BodyState::Thinking, 0.8);
    assert_eq!(org.shown_state(), BodyState::Thinking);
}

#[test]
fn birth_right_after_delegating_does_not_double_the_wave() {
    let mut org = Organism::new(Config::default());
    let mut cells = vec![cell("a", Status::Working)];
    run(&mut org, &cells, BodyState::Idle, 1.0);
    org.delegate();
    cells.push(cell("n", Status::Working).with_new(true));
    run(&mut org, &cells, BodyState::Idle, 1.0 / 60.0);
    assert_eq!(org.waves().len(), 1);
}

#[test]
fn pending_cells_are_dimmed_to_the_pending_alpha() {
    let mut org = Organism::new(Config::default());
    let cells = vec![cell("p", Status::Pending), cell("w", Status::Working)];
    run(&mut org, &cells, BodyState::Idle, 1.0);
    let list = org.shape(&Scene::layout(640.0, 1.0));
    let label_alpha = |id: &str| {
        list.prims
            .iter()
            .find_map(|p| match p {
                Prim::Text { text, color, .. } if text.as_str() == id => Some(color.a),
                _ => None,
            })
            .unwrap()
    };
    let ratio = label_alpha("p") / label_alpha("w");
    assert!((ratio - 0.6).abs() < 1e-3, "{ratio}");
}

#[test]
fn more_than_twelve_cells_fold_the_oldest_finished_ones_into_a_plus_n_cell() {
    let mut org = Organism::new(Config::default());
    let mut cells: Vec<_> = (0..8)
        .map(|i| cell(&format!("done{i}"), Status::Done))
        .collect();
    cells.extend((0..3).map(|i| cell(&format!("wait{i}"), Status::Waiting)));
    cells.extend((0..4).map(|i| cell(&format!("err{i}"), Status::Error)));
    // 15 listed: fold 4 done cells into one aggregate -> 12 on the ring.
    run(&mut org, &cells, BodyState::Idle, 2.0);
    let live: Vec<_> = org
        .cells()
        .iter()
        .filter(|c| matches!(c.life, Life::Alive))
        .collect();
    assert_eq!(live.len(), 12);
    let fold = live
        .iter()
        .find(|c| c.id().as_str() == organism::FOLD_ID)
        .expect("aggregate");
    assert_eq!(&*fold.input.label, "+4");
    assert_eq!(org.folded().len(), 4);
    assert!(org.folded().iter().all(|f| f.as_str().starts_with("done")));
    assert!(
        live.iter().any(|c| c.id().as_str() == "wait0")
            && live.iter().any(|c| c.id().as_str() == "err3")
    );
}

#[test]
fn waiting_and_error_cells_are_never_folded() {
    let mut org = Organism::new(Config::default());
    let mut cells: Vec<_> = (0..8)
        .map(|i| cell(&format!("w{i}"), Status::Waiting))
        .collect();
    cells.extend((0..7).map(|i| cell(&format!("e{i}"), Status::Error)));
    run(&mut org, &cells, BodyState::Idle, 2.0);
    assert!(org.folded().is_empty());
    assert_eq!(org.cells().len(), 15);
}

#[test]
fn phase_is_stable_per_id_across_instances() {
    let cells = vec![
        cell("refactor-gpui", Status::Working),
        cell("tests", Status::Done),
    ];
    let mut a = Organism::new(Config {
        parity: false,
        seed: 1,
    });
    let mut b = Organism::new(Config {
        parity: false,
        seed: 99,
    });
    run(&mut a, &cells, BodyState::Idle, 0.1);
    run(&mut b, &cells, BodyState::Idle, 0.1);
    for (x, y) in a.cells().iter().zip(b.cells()) {
        assert_eq!(x.phase, y.phase);
        assert_eq!(x.flow, y.flow);
    }
}

#[test]
fn shape_output_is_reused_without_growth() {
    let mut org = Organism::new(Config::default());
    let cells: Vec<_> = (0..6)
        .map(|i| cell(&format!("c{i}"), Status::Working))
        .collect();
    run(&mut org, &cells, BodyState::Thinking, 1.0);
    let scene = Scene::layout(640.0, 1.0);
    let mut list = DrawList::new();
    org.shape_into(&scene, &mut list);
    let (cp, cd, cq) = (
        list.points.capacity(),
        list.discs.capacity(),
        list.prims.capacity(),
    );
    for _ in 0..50 {
        org.step(DT, &Inputs::new(BodyState::Thinking, &cells));
        org.shape_into(&scene, &mut list);
    }
    assert_eq!(
        (
            list.points.capacity(),
            list.discs.capacity(),
            list.prims.capacity()
        ),
        (cp, cd, cq)
    );
}
