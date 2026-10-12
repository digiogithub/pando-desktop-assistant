//! GPUI-free state of the organism canvas: the simulation, its inputs and the frame loop.

use std::time::{Duration, Instant};

use organism::{
    BodyState, CellId, CellInput, CellSemantics, Config, DrawList, Inputs, Organism, Scene,
};

/// Seconds without a frame after which the clock restarts (window hidden / suspended).
const RESUME_GAP: Duration = Duration::from_millis(500);

/// Rolling frame-time statistics (PDA-T-0043).
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    /// Last `N` frame intervals in ms (ring buffer).
    intervals_ms: Vec<f32>,
    /// Last `N` CPU costs of `advance + paint` in ms.
    cpu_ms: Vec<f32>,
    next: usize,
    /// Prims and points of the last frame.
    pub prims: usize,
    pub points: usize,
    /// Path passes (`paint_path` calls) of the last frame.
    pub paths: usize,
    /// Total frames painted.
    pub frames: u64,
}

const WINDOW: usize = 120;

impl FrameStats {
    /// Records one frame.
    pub fn record(&mut self, interval_ms: f32, cpu_ms: f32) {
        if self.intervals_ms.len() < WINDOW {
            self.intervals_ms.push(interval_ms);
            self.cpu_ms.push(cpu_ms);
        } else {
            self.intervals_ms[self.next] = interval_ms;
            self.cpu_ms[self.next] = cpu_ms;
        }
        self.next = (self.next + 1) % WINDOW;
        self.frames += 1;
    }

    fn percentile(v: &[f32], p: f32) -> f32 {
        if v.is_empty() {
            return 0.0;
        }
        let mut s = v.to_vec();
        s.sort_by(f32::total_cmp);
        s[((s.len() - 1) as f32 * p) as usize]
    }

    /// Median / 95th percentile frame interval in ms.
    #[must_use]
    pub fn interval_p50_p95(&self) -> (f32, f32) {
        (
            Self::percentile(&self.intervals_ms, 0.5),
            Self::percentile(&self.intervals_ms, 0.95),
        )
    }

    /// Median / 95th percentile CPU cost in ms.
    #[must_use]
    pub fn cpu_p50_p95(&self) -> (f32, f32) {
        (
            Self::percentile(&self.cpu_ms, 0.5),
            Self::percentile(&self.cpu_ms, 0.95),
        )
    }

    /// Frames per second from the median interval.
    #[must_use]
    pub fn fps(&self) -> f32 {
        let (p50, _) = self.interval_p50_p95();
        if p50 > 0.0 { 1000.0 / p50 } else { 0.0 }
    }
}

/// Everything the canvas needs between frames.
#[derive(Debug)]
pub struct Core {
    pub organism: Organism,
    pub cells: Vec<CellInput>,
    pub state: BodyState,
    pub mic_rms: Option<f32>,
    pub tts_rms: Option<f32>,
    /// Pointer in canvas logical px relative to the square origin, `None` outside.
    pub pointer_px: Option<[f32; 2]>,
    pub selected: Option<CellId>,
    /// Keyboard focus ring target.
    pub focused: Option<CellId>,
    pub reduced_motion: bool,
    pub scene: Scene,
    pub draw: DrawList,
    pub semantics: Vec<CellSemantics>,
    pub stats: FrameStats,
    last: Option<Instant>,
    /// Top-left of the canvas square in window px (set while painting).
    pub origin_px: [f32; 2],
    /// Top-left of the whole canvas element in window px.
    pub view_origin_px: [f32; 2],
    /// Canvas square in logical px (side) of the last frame.
    pub side_px: f32,
}

impl Core {
    #[must_use]
    pub fn new() -> Self {
        Self {
            organism: Organism::new(Config::default()),
            cells: Vec::new(),
            state: BodyState::Idle,
            mic_rms: None,
            tts_rms: None,
            pointer_px: None,
            selected: None,
            focused: None,
            reduced_motion: false,
            scene: Scene::layout(640.0, 1.0),
            draw: DrawList::new(),
            semantics: Vec::new(),
            stats: FrameStats::default(),
            last: None,
            origin_px: [0.0, 0.0],
            view_origin_px: [0.0, 0.0],
            side_px: 640.0,
        }
    }

    /// Time of the previous frame.
    #[must_use]
    pub fn last_frame(&self) -> Option<Instant> {
        self.last
    }

    /// Forget the previous frame time so the next `advance` has `dt = 0` (window shown again).
    pub fn reset_clock(&mut self) {
        self.last = None;
    }

    /// Steps the simulation to `now` for a canvas square of `side_px` and rebuilds the
    /// draw list and semantics. Returns the `dt` used.
    pub fn advance(&mut self, now: Instant, side_px: f32, scale_factor: f32) -> f64 {
        let dt = match self.last {
            Some(prev) if now.duration_since(prev) < RESUME_GAP => {
                now.duration_since(prev).as_secs_f64()
            }
            _ => 0.0,
        };
        self.last = Some(now);
        self.side_px = side_px.max(1.0);
        if (self.scene.size_px - self.side_px).abs() > 0.5
            || (self.scene.scale_factor - scale_factor).abs() > f32::EPSILON
        {
            self.scene = Scene::layout(self.side_px, scale_factor);
        }
        let pointer = self.pointer_px.map(|p| {
            [
                f64::from(p[0] / self.side_px),
                f64::from(p[1] / self.side_px),
            ]
        });
        let mut inputs = Inputs::new(self.state, &self.cells);
        inputs.mic_rms = self.mic_rms;
        inputs.tts_rms = self.tts_rms;
        inputs.pointer = pointer;
        inputs.selected = self.selected.clone();
        inputs.reduced_motion = self.reduced_motion;
        inputs.canvas_px = self.side_px;
        self.organism.step(dt, &inputs);
        self.organism.shape_into(&self.scene, &mut self.draw);
        self.organism
            .semantics_into(&self.scene, &mut self.semantics);
        dt
    }

    /// Moves the keyboard focus by `delta` cells in ring order. Returns `false` when it would
    /// leave the list (so Tab can move on to the next widget).
    pub fn move_focus(&mut self, delta: isize) -> bool {
        let n = self.semantics.len() as isize;
        if n == 0 {
            return false;
        }
        let cur = self
            .focused
            .as_ref()
            .and_then(|f| self.semantics.iter().position(|s| &s.id == f))
            .map(|i| i as isize);
        let next = match cur {
            None => {
                if delta > 0 {
                    0
                } else {
                    n - 1
                }
            }
            Some(i) => i + delta,
        };
        if next < 0 || next >= n {
            // Entering from outside lands on the edge; leaving past it releases the focus.
            if cur.is_some() {
                self.focused = None;
            }
            return false;
        }
        self.focused = Some(self.semantics[next as usize].id.clone());
        true
    }

    /// Focus the first (or last) cell.
    pub fn focus_edge(&mut self, last: bool) {
        self.focused = if last {
            self.semantics.last()
        } else {
            self.semantics.first()
        }
        .map(|s| s.id.clone());
    }

    /// The focused cell when it can be selected (not the `+N` aggregate).
    #[must_use]
    pub fn focused_selectable(&self) -> Option<CellId> {
        let f = self.focused.as_ref()?;
        self.semantics
            .iter()
            .find(|s| &s.id == f && !s.aggregate)
            .map(|s| s.id.clone())
    }

    /// The cell under a point in canvas logical px.
    #[must_use]
    pub fn hit_test(&self, p: [f32; 2]) -> Option<CellId> {
        self.organism.hit_test(&self.scene, p)
    }
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use organism::{Engine, Status};

    #[test]
    fn advance_builds_a_draw_list_and_clamps_the_resume_gap() {
        let mut core = Core::new();
        core.cells = vec![CellInput::new("a", Engine::Pando, Status::Working)];
        let t0 = Instant::now();
        assert_eq!(core.advance(t0, 400.0, 1.0), 0.0);
        assert!(!core.draw.prims.is_empty());
        let dt = core.advance(t0 + Duration::from_millis(16), 400.0, 1.0);
        assert!((dt - 0.016).abs() < 1e-6);
        // A long gap (hidden window) restarts the clock.
        assert_eq!(core.advance(t0 + Duration::from_secs(30), 400.0, 1.0), 0.0);
        core.reset_clock();
        assert_eq!(core.advance(t0 + Duration::from_secs(31), 400.0, 1.0), 0.0);
    }

    #[test]
    fn semantics_follow_the_cells() {
        let mut core = Core::new();
        core.cells = vec![
            CellInput::new("a", Engine::Pando, Status::Working),
            CellInput::new("b", Engine::Cli, Status::Done),
        ];
        let t0 = Instant::now();
        for i in 0..40 {
            core.advance(t0 + Duration::from_millis(16 * i), 500.0, 1.0);
        }
        assert_eq!(core.semantics.len(), 2);
    }

    #[test]
    fn keyboard_focus_cycles_through_cells_and_releases_at_the_ends() {
        let mut core = Core::new();
        core.cells = vec![
            CellInput::new("a", Engine::Pando, Status::Working),
            CellInput::new("b", Engine::Cli, Status::Done),
        ];
        let t0 = Instant::now();
        for i in 0..40 {
            core.advance(t0 + Duration::from_millis(16 * i), 500.0, 1.0);
        }
        assert!(core.move_focus(1));
        let first = core.focused.clone();
        assert!(core.move_focus(1));
        assert_ne!(core.focused, first);
        assert!(!core.move_focus(1), "past the last cell releases the focus");
        assert!(core.focused.is_none());
        assert!(core.move_focus(-1), "shift-tab enters from the end");
        assert!(core.focused_selectable().is_some());
        core.focus_edge(false);
        assert_eq!(core.focused, first);
    }

    /// `cargo test -p app --release core_frame_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "timing probe, run in release"]
    fn core_frame_cost() {
        for side in [300.0_f32, 640.0, 1000.0, 1600.0] {
            let mut core = Core::new();
            core.cells = (0..14)
                .map(|i| CellInput::new(&format!("c{i}"), Engine::Pando, Status::Working))
                .collect();
            let t0 = Instant::now();
            for i in 0..120 {
                core.advance(t0 + Duration::from_millis(16 * i), side, 1.0);
            }
            let start = Instant::now();
            for i in 120..720 {
                core.advance(t0 + Duration::from_millis(16 * i), side, 1.0);
            }
            let per = start.elapsed().as_secs_f64() * 1000.0 / 600.0;
            println!(
                "side {side}: step+shape+semantics {per:.3} ms/frame, {} prims, {} pts",
                core.draw.prims.len(),
                core.draw.points.len()
            );
        }
    }

    #[test]
    fn stats_report_percentiles() {
        let mut s = FrameStats::default();
        for i in 0..100 {
            s.record(16.0 + (i % 10) as f32, 1.0);
        }
        let (p50, p95) = s.interval_p50_p95();
        assert!(p50 >= 16.0 && p95 >= p50);
        assert!(s.fps() > 30.0);
    }
}
