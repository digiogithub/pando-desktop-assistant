//! The first-run assistant window.

use rust_i18n::t;

use super::{
    HooksInstaller, MicProbe, ModelProvisioner, NeedState, NoHooks, NoMic, NoModels, Step, Wizard,
    human_bytes, missing_bytes,
};
use crate::organism_view::tr;
use crate::settings_view::{self, model::Change};
use crate::tokio_bridge;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, InputState};
use crate::ui::theme::palette;
use crate::ui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window, div, h_flex, px, rgb, rgba, v_flex,
};

/// Opens (or focuses) the assistant.
pub fn open(cx: &mut crate::ui::App) {
    crate::windows::open(
        cx,
        "first-run",
        t!("firstrun.title").to_string(),
        (640.0, 520.0),
        |window, cx| cx.new(|cx| FirstRunView::new(window, cx)),
    );
}

/// The wizard view.
#[allow(missing_debug_implementations)]
pub struct FirstRunView {
    wizard: Wizard,
    url: Entity<InputState>,
    status: Option<String>,
    models: Box<dyn ModelProvisioner>,
    mic: Box<dyn MicProbe>,
    hooks: Box<dyn HooksInstaller>,
}

impl FirstRunView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = settings_view::current(cx)
            .and_then(|s| s.pando_url)
            .unwrap_or_default();
        let url = cx.new(|cx| {
            let mut st = InputState::new(window, cx)
                .placeholder(t!("pando.empty.url_placeholder").to_string());
            if !current.is_empty() {
                st.set_value(current, window, cx);
            }
            st
        });
        Self {
            wizard: Wizard::new(),
            url,
            status: None,
            models: Box::new(NoModels),
            mic: Box::new(NoMic),
            hooks: Box::new(NoHooks),
        }
    }

    fn detect(&mut self, cx: &mut Context<Self>) {
        let typed = self.url.read(cx).value().to_string();
        self.status = Some(t!("firstrun.connection.probing").to_string());
        let task = tokio_bridge::spawn(cx, async move {
            let mut d = pando_client::Discovery::default();
            if !typed.trim().is_empty() {
                d.manual_url = Some(typed.trim().to_owned());
            }
            d.discover().await.map(|s| s.info.version)
        });
        cx.spawn(async move |this, cx| {
            let out = task.await;
            this.update(cx, |this, cx| {
                this.status = Some(match out {
                    Ok(Ok(version)) => {
                        t!("firstrun.connection.found", version = version).to_string()
                    }
                    _ => t!("firstrun.connection.not_found").to_string(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.wizard.step() == Step::Connection {
            let typed = self.url.read(cx).value().to_string();
            let url = Some(typed).filter(|u| !u.trim().is_empty());
            if let Err(e) = settings_view::commit(cx, Change::PandoUrl(url)) {
                self.status = Some(e);
                cx.notify();
                return;
            }
        }
        self.wizard.complete_step();
        self.after(window, cx);
    }

    fn after(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.status = None;
        if self.wizard.finished() {
            self.finish(window, cx);
        }
        cx.notify();
    }

    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Skipped or completed, the assistant does not reopen by itself.
        if let Some((settings, path)) = crate::shell::runtime::settings_handle(cx) {
            let mut s = settings.borrow_mut();
            s.first_run_done = true;
            if let Err(e) = s.save(&path) {
                tracing::warn!("cannot save settings: {e}");
            }
        }
        window.remove_window();
    }

    fn step_body(&self, cx: &mut Context<Self>) -> gpui_any::Any {
        let weak = cx.weak_entity();
        match self.wizard.step() {
            Step::Connection => {
                let w = weak.clone();
                v_flex()
                    .gap(px(8.0))
                    .child(Input::new(&self.url))
                    .child(
                        Button::new("fr-detect")
                            .label(t!("firstrun.connection.detect").to_string())
                            .on_click(move |_, _, cx| {
                                w.update(cx, |t, cx| t.detect(cx)).ok();
                            }),
                    )
                    .into_any_element()
            }
            Step::Microphone => div()
                .child(if self.mic.available() {
                    t!("firstrun.microphone.available").to_string()
                } else {
                    t!("firstrun.microphone.pending").to_string()
                })
                .into_any_element(),
            Step::Voice => {
                let needs = self.models.required();
                if needs.is_empty() {
                    div()
                        .child(t!("firstrun.voice.none").to_string())
                        .into_any_element()
                } else {
                    v_flex()
                        .gap(px(4.0))
                        .children(needs.iter().map(|m| {
                            let state = match self.models.state(&m.id) {
                                NeedState::Ready => t!("firstrun.voice.ready").to_string(),
                                NeedState::Downloading(p) => format!("{p} %"),
                                NeedState::Missing => t!("firstrun.voice.missing").to_string(),
                                NeedState::Failed(e) => e,
                            };
                            div().child(format!(
                                "{} · {} · {} · {state}",
                                m.label,
                                human_bytes(m.bytes),
                                m.licence
                            ))
                        }))
                        .child(
                            div().child(
                                t!(
                                    "firstrun.voice.consent",
                                    size = human_bytes(missing_bytes(self.models.as_ref()))
                                )
                                .to_string(),
                            ),
                        )
                        .into_any_element()
                }
            }
            Step::Hotkey => {
                let shortcut = settings_view::current(cx)
                    .map(|s| s.shell.ptt_shortcut)
                    .unwrap_or_default();
                let status = ["global-hotkey", "portal"]
                    .iter()
                    .filter_map(|n| {
                        crate::shell::runtime::backend_status(cx, n).map(|s| format!("{n}: {s}"))
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                v_flex()
                    .gap(px(4.0))
                    .child(div().font_family(crate::fonts::FONT_MONO).child(shortcut))
                    .child(div().text_color(rgba(0xF4F1E8A0)).child(status))
                    .into_any_element()
            }
            Step::ClaudeHooks => div()
                .child(if self.hooks.available() {
                    t!("firstrun.hooks.available").to_string()
                } else {
                    t!("firstrun.hooks.pending").to_string()
                })
                .into_any_element(),
            Step::Hello => div()
                .child(t!("firstrun.hello.body").to_string())
                .into_any_element(),
        }
    }
}

mod gpui_any {
    pub use crate::ui::AnyElement as Any;
}

impl Render for FirstRunView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let step = self.wizard.step();
        let (n, total) = self.wizard.position();
        let body = self.step_body(cx);
        let (w_back, w_skip, w_next, w_all) = (weak.clone(), weak.clone(), weak.clone(), weak);
        let last = n == total;
        v_flex()
            .id("first-run")
            .size_full()
            .bg(rgb(palette::BOSQUE))
            .text_color(rgb(palette::MARFIL))
            .p(px(28.0))
            .gap(px(14.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_family(crate::fonts::FONT_MONO)
                    .text_color(rgba(0xF4F1E8A0))
                    .child(t!("firstrun.progress", n = n, total = total).to_string()),
            )
            .child(
                div()
                    .text_size(px(22.0))
                    .child(tr(&format!("firstrun.step.{}.title", step.id()))),
            )
            .child(
                div()
                    .text_color(rgba(0xF4F1E8C0))
                    .child(tr(&format!("firstrun.step.{}.body", step.id()))),
            )
            .child(div().flex_1().child(body))
            .children(
                self.status
                    .clone()
                    .map(|s| div().text_size(px(12.0)).child(s)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("fr-back")
                            .ghost()
                            .label(t!("firstrun.back").to_string())
                            .on_click(move |_, _, cx| {
                                w_back
                                    .update(cx, |t, cx| {
                                        t.wizard.back();
                                        t.status = None;
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("fr-all")
                            .ghost()
                            .label(t!("firstrun.skip_all").to_string())
                            .on_click(move |_, window, cx| {
                                w_all
                                    .update(cx, |t, cx| {
                                        t.wizard.skip_all();
                                        t.after(window, cx);
                                    })
                                    .ok();
                            }),
                    )
                    .child(
                        Button::new("fr-skip")
                            .label(t!("firstrun.skip").to_string())
                            .on_click(move |_, window, cx| {
                                w_skip
                                    .update(cx, |t, cx| {
                                        t.wizard.skip_step();
                                        t.after(window, cx);
                                    })
                                    .ok();
                            }),
                    )
                    .child(
                        Button::new("fr-next")
                            .primary()
                            .label(
                                if last {
                                    t!("firstrun.finish")
                                } else {
                                    t!("firstrun.next")
                                }
                                .to_string(),
                            )
                            .on_click(move |_, window, cx| {
                                w_next.update(cx, |t, cx| t.next(window, cx)).ok();
                            }),
                    ),
            )
    }
}
