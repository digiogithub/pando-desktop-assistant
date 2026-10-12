//! Demo window (`--demo`, PDA-T-0040): the organism driven by scripted fake agents with a
//! state selector, a legend and the frame-time overlay. Needs neither Pando nor audio.

use std::time::{Duration, Instant};

use organism::{BodyState, CellId, tokens};
use rust_i18n::t;

use super::{OrganismEvent, OrganismView};
use crate::ui::button::Button;
use crate::ui::frameless::window_border;
use crate::ui::paint::KeyDownEvent;
use crate::ui::theme::palette;
use crate::ui::{
    AppContext as _, Context, Entity, FocusHandle, Focusable, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Selectable as _, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, Window, div, h_flex, px, rgb, rgba, v_flex,
};
use crate::views::title_bar::AppTitleBar;

use super::demo_source::DemoSource;

/// Period of the script tick.
const TICK: Duration = Duration::from_millis(200);

/// Maps the demo keys `1`-`5` to body states.
#[must_use]
pub fn state_for_key(key: &str) -> Option<BodyState> {
    match key {
        "1" => Some(BodyState::Idle),
        "2" => Some(BodyState::Listening),
        "3" => Some(BodyState::Thinking),
        "4" => Some(BodyState::Speaking),
        "5" => Some(BodyState::Delegating),
        _ => None,
    }
}

/// Localized name of a body state.
#[must_use]
pub fn state_name(s: BodyState) -> String {
    let id = match s {
        BodyState::Idle => "idle",
        BodyState::Listening => "listening",
        BodyState::Thinking => "thinking",
        BodyState::Speaking => "speaking",
        BodyState::Delegating => "delegating",
    };
    super::tr(&format!("organism.state.{id}"))
}

/// Root view of the demo window.
#[allow(missing_debug_implementations)]
pub struct DemoView {
    organism: Entity<OrganismView>,
    source: DemoSource,
    state: BodyState,
    selected_label: Option<String>,
    selected: Option<CellId>,
    focus: FocusHandle,
    _tick: Task<()>,
    _sub: Subscription,
}

impl Focusable for DemoView {
    fn focus_handle(&self, _cx: &crate::ui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DemoView {
    /// Builds the demo; `reduced_motion` follows the app setting.
    pub fn new(window: &mut Window, cx: &mut Context<Self>, reduced_motion: bool) -> Self {
        let source = DemoSource::new(42);
        let organism = cx.new(|cx| OrganismView::new(window, cx));
        organism.update(cx, |o, cx| {
            o.set_cells(source.cells().to_vec(), cx);
            o.set_reduced_motion(reduced_motion, cx);
            o.set_show_timings(true, cx);
        });
        let sub = cx.subscribe(&organism, |this, _, ev: &OrganismEvent, cx| {
            match ev {
                OrganismEvent::CellSelected(id) => {
                    this.selected = Some(id.clone());
                    this.selected_label = this
                        .source
                        .cells()
                        .iter()
                        .find(|c| &c.id == id)
                        .map(|c| c.label.to_string());
                }
                OrganismEvent::Deselected => {
                    this.selected = None;
                    this.selected_label = None;
                }
            }
            cx.notify();
        });
        let tick = cx.spawn(async move |this, cx| {
            let mut last = Instant::now();
            loop {
                cx.background_executor().timer(TICK).await;
                let now = Instant::now();
                let dt = now.duration_since(last).as_secs_f64();
                last = now;
                if this.update(cx, |this, cx| this.tick(dt, cx)).is_err() {
                    break;
                }
            }
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            organism,
            source,
            state: BodyState::Idle,
            selected_label: None,
            selected: None,
            focus,
            _tick: tick,
            _sub: sub,
        }
    }

    fn push(&mut self, delegate: bool, cx: &mut Context<Self>) {
        let cells = self.source.cells().to_vec();
        self.organism.update(cx, |o, cx| {
            o.set_cells(cells, cx);
            if delegate {
                o.delegate(cx);
            }
        });
        cx.notify();
    }

    fn tick(&mut self, dt: f64, cx: &mut Context<Self>) {
        let ch = self.source.tick(dt);
        if ch.changed {
            self.push(ch.spawned, cx);
        }
    }

    /// Sets the body state.
    pub fn set_state(&mut self, s: BodyState, cx: &mut Context<Self>) {
        self.state = s;
        self.organism.update(cx, |o, cx| {
            if s == BodyState::Delegating {
                o.delegate(cx);
            } else {
                o.set_state(s, cx);
            }
        });
        cx.notify();
    }

    fn add(&mut self, cx: &mut Context<Self>) {
        if self.source.spawn() {
            self.push(true, cx);
        }
    }

    fn remove(&mut self, cx: &mut Context<Self>) {
        if self.source.remove_oldest().is_some() {
            self.push(false, cx);
        }
    }

    fn cycle(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected.clone()
            && self.source.cycle_status(&id)
        {
            self.push(false, cx);
        }
    }

    fn toggle_timings(&mut self, cx: &mut Context<Self>) {
        self.organism.update(cx, |o, cx| {
            let on = !o.show_timings();
            o.set_show_timings(on, cx);
        });
    }

    fn on_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        if let Some(s) = state_for_key(key) {
            self.set_state(s, cx);
            return;
        }
        match key {
            "n" => self.add(cx),
            "x" => self.remove(cx),
            "s" => self.cycle(cx),
            "f" => self.toggle_timings(cx),
            _ => {}
        }
    }
}

fn token_color(c: tokens::Color) -> Hsla {
    rgba((c.rgb << 8) | u32::from(c.a)).into()
}

fn marfil(alpha: palette::Rgba) -> Hsla {
    rgba(alpha.as_u32()).into()
}

fn section(title: String) -> impl IntoElement {
    div()
        .mt(px(14.0))
        .mb(px(4.0))
        .text_size(px(11.0))
        .font_family(crate::fonts::FONT_MONO)
        .text_color(marfil(palette::MARFIL_ALPHA_62))
        .child(title)
}

impl Render for DemoView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let state_buttons = BodyState::ALL.iter().enumerate().map(|(i, s)| {
            let weak = weak.clone();
            let s = *s;
            Button::new(("demo-state", i))
                .label(format!("{} {}", i + 1, state_name(s)))
                .selected(self.state == s)
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| this.set_state(s, cx)).ok();
                })
        });
        let action =
            |id: &'static str, label: String, f: fn(&mut DemoView, &mut Context<DemoView>)| {
                let weak = weak.clone();
                Button::new(id).label(label).on_click(move |_, _, cx| {
                    weak.update(cx, f).ok();
                })
            };
        let engines = tokens::ENGINES.iter().map(|e| {
            let dot = div()
                .size(px(10.0))
                .rounded_full()
                .border_2()
                .border_color(token_color(e.nucleus));
            let dot = if e.fill {
                dot.bg(token_color(e.nucleus))
            } else {
                dot
            };
            h_flex()
                .gap(px(8.0))
                .items_center()
                .child(dot)
                .child(super::tr(&format!("organism.engine.{}", e.id)))
        });
        let statuses = tokens::STATUSES.iter().map(|s| {
            h_flex()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .w(px(14.0))
                        .text_color(marfil(palette::MARFIL_ALPHA_62))
                        .child(s.badge.unwrap_or("·")),
                )
                .child(super::tr(&format!("organism.status.{}", s.id)))
        });
        let selected = match &self.selected_label {
            Some(l) => t!("organism.demo.selected", label = l).to_string(),
            None => t!("organism.demo.none_selected").to_string(),
        };
        let panel = v_flex()
            .id("demo-panel")
            .overflow_y_scroll()
            .w(px(280.0))
            .flex_shrink_0()
            .h_full()
            .p(px(16.0))
            .gap(px(6.0))
            .border_l_1()
            .border_color(marfil(palette::MARFIL_ALPHA_12))
            .text_size(px(12.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .child(t!("organism.demo.title").to_string()),
            )
            .child(section(t!("organism.demo.states").to_string()))
            .child(v_flex().gap(px(4.0)).children(state_buttons))
            .child(div().h(px(8.0)))
            .child(action(
                "demo-add",
                t!("organism.demo.add").to_string(),
                Self::add,
            ))
            .child(action(
                "demo-remove",
                t!("organism.demo.remove").to_string(),
                Self::remove,
            ))
            .child(action(
                "demo-cycle",
                t!("organism.demo.cycle").to_string(),
                Self::cycle,
            ))
            .child(
                div()
                    .mt(px(8.0))
                    .text_color(marfil(palette::MARFIL_ALPHA_62))
                    .child(selected),
            )
            .child(section(t!("organism.demo.engines").to_string()))
            .child(v_flex().gap(px(4.0)).children(engines))
            .child(section(t!("organism.demo.statuses").to_string()))
            .child(v_flex().gap(px(4.0)).children(statuses));
        let title_bar = AppTitleBar::new().left(
            div()
                .text_size(px(13.0))
                .child(t!("organism.demo.title").to_string()),
        );
        window_border().child(
            v_flex()
                .id("organism-demo")
                .track_focus(&self.focus)
                .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| this.on_key(e, cx)))
                .size_full()
                .bg(rgb(palette::BOSQUE))
                .text_color(rgb(palette::MARFIL))
                .child(title_bar)
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .child(div().flex_1().h_full().child(self.organism.clone()))
                        .child(panel),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_states() {
        assert_eq!(state_for_key("1"), Some(BodyState::Idle));
        assert_eq!(state_for_key("5"), Some(BodyState::Delegating));
        assert_eq!(state_for_key("6"), None);
    }
}
