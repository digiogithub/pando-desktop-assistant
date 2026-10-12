//! The organism canvas: a GPUI entity that owns the simulation, steps it every frame and
//! paints the resulting draw list with GPUI paths (PDA-T-0038, ADR-0006).
//!
//! * `core` - GPUI-free state and frame loop (unit tested).
//! * `geometry` - pure helpers (dashing).
//! * `paint` - draw list to `paint_path` / `shape_line`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use organism::{BodyState, CellId, CellInput};

use rust_i18n::t;

use crate::ui::a11y::Role;
use crate::ui::paint::{MouseButton, MouseDownEvent, MouseMoveEvent, WindowVisibility, canvas};
use crate::ui::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Point, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, px,
};

pub mod adapter;
pub mod core;
pub mod demo;
pub mod demo_source;
pub mod geometry;
pub mod paint;
#[cfg(feature = "raster")]
pub mod raster;
#[cfg(all(test, feature = "raster"))]
mod visual;

pub use core::{Core, FrameStats};

/// Default frame interval while the window is visible but not focused.
pub const INACTIVE_FRAME_INTERVAL: Duration = Duration::from_millis(100);

/// Events the organism emits to its parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrganismEvent {
    /// A cell was selected (click, or keyboard focus + Enter).
    CellSelected(CellId),
    /// Selection cleared (Escape or click on the body).
    Deselected,
}

crate::ui::actions!(
    organism,
    [
        CycleNext, CyclePrev, Activate, Deselect, FocusFirst, FocusLast
    ]
);

/// Key context of the canvas.
pub const KEY_CONTEXT: &str = "Organism";

/// Registers the keyboard bindings of the canvas (call once at startup).
pub fn bind_keys(cx: &mut App) {
    let table: [(&str, Box<dyn crate::ui::Action>); 11] = [
        ("tab", Box::new(CycleNext)),
        ("shift-tab", Box::new(CyclePrev)),
        ("right", Box::new(CycleNext)),
        ("down", Box::new(CycleNext)),
        ("left", Box::new(CyclePrev)),
        ("up", Box::new(CyclePrev)),
        ("enter", Box::new(Activate)),
        ("space", Box::new(Activate)),
        ("escape", Box::new(Deselect)),
        ("home", Box::new(FocusFirst)),
        ("end", Box::new(FocusLast)),
    ];
    let bindings: Vec<_> = table
        .into_iter()
        .filter_map(|(keys, action)| crate::ui::key_binding(keys, action, Some(KEY_CONTEXT)).ok())
        .collect();
    cx.bind_keys(bindings);
}

/// Translates a dynamic key.
#[must_use]
pub fn tr(key: &str) -> String {
    t!(key).to_string()
}

/// Localized accessible name of a cell: "<label>, <engine>, <status>".
#[must_use]
pub fn aria_label(c: &organism::CellSemantics) -> String {
    if c.aggregate {
        return t!("organism.cell.fold_aria", count = c.folded.len()).to_string();
    }
    let engine = tr(&format!("organism.engine.{}", c.engine_id));
    let status = tr(&format!("organism.status.{}", c.status_id));
    t!(
        "organism.cell.aria",
        label = c.label.as_ref(),
        engine = engine,
        status = status
    )
    .to_string()
}

/// Which backend paints the geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderMode {
    /// GPUI paths, or the CPU raster when GPUI reports a software renderer.
    #[default]
    Auto,
    /// Always GPUI paths.
    Gpu,
    /// Always the tiny-skia raster (needs the `raster` feature, else behaves as `Gpu`).
    Raster,
}

impl RenderMode {
    /// Parses `auto` / `gpu` / `raster` (the `PANDO_ORGANISM_RENDER` override).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "gpu" => Some(Self::Gpu),
            "raster" => Some(Self::Raster),
            _ => None,
        }
    }

    /// Whether the raster backend should paint, given the GPU report.
    #[must_use]
    pub fn use_raster(self, software_gpu: bool) -> bool {
        cfg!(feature = "raster")
            && match self {
                Self::Auto => software_gpu,
                Self::Gpu => false,
                Self::Raster => true,
            }
    }
}

/// Minimum time between raster frames (30 fps).
#[cfg(feature = "raster")]
const RASTER_INTERVAL: Duration = Duration::from_millis(33);

/// The last raster frame and when / at which size it was drawn.
#[cfg(feature = "raster")]
#[derive(Default)]
struct RasterState {
    frame: Option<crate::ui::paint::raster_image::Frame>,
    at: Option<Instant>,
    side: u32,
}

/// The organism canvas entity.
#[allow(missing_debug_implementations)]
pub struct OrganismView {
    mode: RenderMode,
    #[cfg(feature = "raster")]
    raster: Rc<RefCell<RasterState>>,
    core: Rc<RefCell<Core>>,
    text: Rc<RefCell<paint::TextCache>>,
    focus: FocusHandle,
    inactive_interval: Duration,
    show_timings: bool,
    _visibility: Option<Subscription>,
}

impl EventEmitter<OrganismEvent> for OrganismView {}

impl Focusable for OrganismView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl OrganismView {
    /// A view that paints nothing until cells / state are set; subscribes to window visibility
    /// so the frame loop stops while hidden and restarts with a fresh clock.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let core = Rc::new(RefCell::new(Core::new()));
        let weak = cx.weak_entity();
        let visibility = {
            let core = core.clone();
            window.observe_window_visibility(move |visibility, _, cx| {
                if visibility == WindowVisibility::Visible {
                    core.borrow_mut().reset_clock();
                    weak.update(cx, |_, cx| cx.notify()).ok();
                }
            })
        };
        let mode = std::env::var("PANDO_ORGANISM_RENDER")
            .ok()
            .and_then(|v| RenderMode::parse(&v))
            .unwrap_or_default();
        Self {
            mode,
            #[cfg(feature = "raster")]
            raster: Rc::default(),
            core,
            text: Rc::default(),
            focus: cx.focus_handle(),
            inactive_interval: INACTIVE_FRAME_INTERVAL,
            show_timings: false,
            _visibility: Some(visibility),
        }
    }

    /// Shared handle to the GPUI-free state (tests, overlays).
    #[must_use]
    pub fn core(&self) -> Rc<RefCell<Core>> {
        self.core.clone()
    }

    /// Replaces the list of cells the organism shows.
    pub fn set_cells(&mut self, cells: Vec<CellInput>, cx: &mut Context<Self>) {
        let mut c = self.core.borrow_mut();
        c.cells = cells;
        // Drop a focus / selection that points at a cell that no longer exists.
        let alive = |id: &CellId, cells: &[CellInput]| cells.iter().any(|x| &x.id == id);
        if c.focused.as_ref().is_some_and(|f| !alive(f, &c.cells)) {
            c.focused = None;
        }
        drop(c);
        cx.notify();
    }

    /// Sets the body state (voice pipeline).
    pub fn set_state(&mut self, state: BodyState, cx: &mut Context<Self>) {
        self.core.borrow_mut().state = state;
        cx.notify();
    }

    /// Starts the Delegating overlay (a task was just handed to an agent).
    pub fn delegate(&mut self, cx: &mut Context<Self>) {
        self.core.borrow_mut().organism.delegate();
        cx.notify();
    }

    /// Mic / TTS levels in `0..=1`; `None` keeps the synthetic envelope.
    pub fn set_levels(&mut self, mic: Option<f32>, tts: Option<f32>) {
        let mut c = self.core.borrow_mut();
        c.mic_rms = mic;
        c.tts_rms = tts;
    }

    /// Reduced motion (settings or OS preference).
    pub fn set_reduced_motion(&mut self, on: bool, cx: &mut Context<Self>) {
        self.core.borrow_mut().reduced_motion = on;
        cx.notify();
    }

    /// Frame interval used while the window is visible but inactive.
    pub fn set_inactive_interval(&mut self, interval: Duration) {
        self.inactive_interval = interval;
    }

    /// Chooses the paint backend.
    pub fn set_render_mode(&mut self, mode: RenderMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    /// Shows the frame-time overlay (PDA-T-0043).
    pub fn set_show_timings(&mut self, on: bool, cx: &mut Context<Self>) {
        self.show_timings = on;
        cx.notify();
    }

    /// Whether the frame-time overlay is on.
    #[must_use]
    pub fn show_timings(&self) -> bool {
        self.show_timings
    }

    /// Selected cell, if any (does not emit an event).
    pub fn set_selected(&mut self, id: Option<CellId>, cx: &mut Context<Self>) {
        self.core.borrow_mut().selected = id;
        cx.notify();
    }

    /// Selects `id` and tells the parent.
    pub fn select(&mut self, id: CellId, cx: &mut Context<Self>) {
        {
            let mut c = self.core.borrow_mut();
            c.selected = Some(id.clone());
            c.focused = Some(id.clone());
        }
        cx.emit(OrganismEvent::CellSelected(id));
        cx.notify();
    }

    /// Clears the selection and tells the parent.
    pub fn deselect(&mut self, cx: &mut Context<Self>) {
        let had = self.core.borrow_mut().selected.take().is_some();
        if had {
            cx.emit(OrganismEvent::Deselected);
        }
        cx.notify();
    }

    fn move_focus(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let moved = self.core.borrow_mut().move_focus(delta);
        if moved {
            cx.notify();
        } else if delta > 0 {
            // Past the last cell: leave the canvas like a normal tab stop.
            window.focus_next(cx);
        } else {
            window.focus_prev(cx);
        }
    }

    fn pointer_to_local(&self, p: Point<Pixels>) -> [f32; 2] {
        let o = self.core.borrow().origin_px;
        [f32::from(p.x) - o[0], f32::from(p.y) - o[1]]
    }
}

/// Paints the geometry with the CPU raster, redrawing the pixmap at most every 33 ms.
#[cfg(feature = "raster")]
fn paint_raster(
    window: &mut Window,
    _bounds: crate::ui::Bounds<Pixels>,
    origin: Point<Pixels>,
    side: f32,
    core: &Core,
    st: &mut RasterState,
) {
    let pixels = raster::fallback_side(side, window.scale_factor());
    let due = st.at.is_none_or(|t| t.elapsed() >= RASTER_INTERVAL) || st.side != pixels;
    if due
        && let Some(pm) = raster::render(&core.draw, side, pixels, None)
        && let Some(frame) =
            crate::ui::paint::raster_image::Frame::from_rgba(pm.width(), pm.height(), pm.data())
    {
        if let Some(old) = st.frame.replace(frame) {
            old.drop_from(window);
        }
        st.at = Some(Instant::now());
        st.side = pixels;
    }
    if let Some(frame) = &st.frame {
        let b = crate::ui::Bounds {
            origin,
            size: crate::ui::size(px(side), px(side)),
        };
        frame.paint(window, b);
    }
}

impl Render for OrganismView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let core = self.core.clone();
        let text = self.text.clone();
        let interval = self.inactive_interval;
        let view = cx.weak_entity();
        let focused = self.focus.is_focused(window);
        let (hovering, cells, offset) = {
            let c = self.core.borrow();
            let hovering = c.semantics.iter().any(|s| s.hovered && !s.aggregate);
            let cells: Vec<_> = c.semantics.clone();
            (hovering, cells, c.origin_px)
        };
        let view_origin = self.core.borrow().view_origin_px;
        let (dx, dy) = (offset[0] - view_origin[0], offset[1] - view_origin[1]);
        let overlays = cells.into_iter().enumerate().map(|(i, s)| {
            let b = s.bounds;
            let mut el = div()
                .id(("organism-cell", i))
                .absolute()
                .left(px(dx + b.x))
                .top(px(dy + b.y))
                .w(px(b.w))
                .h(px(b.h))
                .role(Role::Button)
                .aria_label(aria_label(&s))
                .aria_selected(s.selected);
            if !s.aggregate {
                let id = s.id.clone();
                el = el.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.select(id.clone(), cx);
                    }),
                );
            }
            el
        });
        let mut root = div()
            .id("organism")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .tab_stop(true)
            .relative()
            .size_full()
            .role(Role::Group)
            .aria_label(t!("organism.aria").to_string())
            .on_action(cx.listener(|this, _: &CycleNext, window, cx| {
                this.move_focus(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CyclePrev, window, cx| {
                this.move_focus(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusFirst, _, cx| {
                this.core.borrow_mut().focus_edge(false);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusLast, _, cx| {
                this.core.borrow_mut().focus_edge(true);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Activate, _, cx| {
                let id = this.core.borrow().focused_selectable();
                if let Some(id) = id {
                    this.select(id, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Deselect, _, cx| {
                this.deselect(cx);
            }))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                let p = this.pointer_to_local(e.position);
                this.core.borrow_mut().pointer_px = Some(p);
                cx.notify();
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered {
                    this.core.borrow_mut().pointer_px = None;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    let p = this.pointer_to_local(e.position);
                    let hit = this.core.borrow().hit_test(p);
                    match hit {
                        Some(id) if id.as_str() != organism::FOLD_ID => this.select(id, cx),
                        Some(_) => {}
                        None => this.deselect(cx),
                    }
                }),
            );
        if hovering {
            root = root.cursor_pointer();
        }
        let show_timings = self.show_timings;
        let mode = self.mode;
        #[cfg(feature = "raster")]
        let raster = self.raster.clone();
        root.child(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    let started = Instant::now();
                    let (origin, side) = paint::square_in(bounds);
                    let mut c = core.borrow_mut();
                    c.origin_px = [f32::from(origin.x), f32::from(origin.y)];
                    c.view_origin_px = [f32::from(bounds.origin.x), f32::from(bounds.origin.y)];
                    let prev = c.last_frame();
                    c.advance(started, side, window.scale_factor());
                    let software = window.gpu_specs().is_some_and(|g| g.is_software_emulated);
                    let mut counts = paint::PaintCounts::default();
                    if mode.use_raster(software) {
                        #[cfg(feature = "raster")]
                        paint_raster(window, bounds, origin, side, &c, &mut raster.borrow_mut());
                    } else {
                        counts = paint::paint_paths(window, origin, &c.draw);
                    }
                    if focused {
                        paint::paint_focus_ring(window, origin, &c, &mut counts);
                    }
                    paint::paint_text(
                        window,
                        cx,
                        origin,
                        &c.draw,
                        &mut text.borrow_mut(),
                        &mut counts,
                    );
                    if show_timings {
                        paint::paint_timings(window, cx, bounds, &c.stats, &mut text.borrow_mut());
                    }
                    let interval_ms =
                        prev.map_or(0.0, |p| started.duration_since(p).as_secs_f32() * 1000.0);
                    let cpu_ms = started.elapsed().as_secs_f32() * 1000.0;
                    c.stats.record(interval_ms, cpu_ms);
                    c.stats.prims = c.draw.prims.len();
                    c.stats.points = c.draw.points.len();
                    c.stats.paths = counts.paths;
                    drop(c);
                    if !window.is_visible() {
                        return;
                    }
                    if window.is_window_active() {
                        window.request_animation_frame();
                    } else {
                        let view = view.clone();
                        cx.spawn(async move |cx| {
                            cx.background_executor().timer(interval).await;
                            view.update(cx, |_, cx| cx.notify()).ok();
                        })
                        .detach();
                    }
                },
            )
            .size_full(),
        )
        .children(overlays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use organism::{Engine, Status};

    #[gpui_test]
    fn paints_frames_with_cells(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            crate::ui::kit_theme::install(cx);
        });
        let window = cx.add_window(OrganismView::new);
        let view = window.root(cx).expect("root view");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        view.update(&mut cx, |v, cx| {
            v.set_cells(
                vec![
                    CellInput::new("a", Engine::Pando, Status::Working),
                    CellInput::new("b", Engine::ClaudeAcp, Status::Waiting),
                ],
                cx,
            );
            v.set_state(BodyState::Thinking, cx);
        });
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let core = view.read_with(&cx, |v, _| v.core());
        let core = core.borrow();
        assert!(core.stats.frames >= 1, "no frame painted");
        assert!(core.stats.paths > 0, "no path painted");
        assert!(core.stats.paths < 400, "{} paths", core.stats.paths);
        assert_eq!(core.semantics.len(), 2);
    }
}
