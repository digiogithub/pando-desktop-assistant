//! Rendering of the settings window.

use rust_i18n::t;

use super::model::{Change, SECTIONS};
use crate::organism_view::tr;
use crate::shell::{autostart, portal, runtime};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, InputState};
use crate::ui::switch::Switch;
use crate::ui::theme::palette;
use crate::ui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Selectable as _, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, div, h_flex, px, rgb, rgba, v_flex,
};

/// The settings window.
#[allow(missing_debug_implementations)]
pub struct SettingsView {
    active: usize,
    url: Entity<InputState>,
    shortcut: Entity<InputState>,
    update_url: Entity<InputState>,
    /// Result of the last edit (error text or restart notice).
    message: Option<(bool, String)>,
}

impl SettingsView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let s = super::current(cx).unwrap_or_default();
        let mk = |value: String, ph: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut st = InputState::new(window, cx).placeholder(ph);
                if !value.is_empty() {
                    st.set_value(value, window, cx);
                }
                st
            })
        };
        Self {
            active: 0,
            url: mk(
                s.pando_url.clone().unwrap_or_default(),
                t!("pando.empty.url_placeholder").to_string(),
                window,
                cx,
            ),
            shortcut: mk(
                s.shell.ptt_shortcut.clone(),
                "ctrl+alt+space".into(),
                window,
                cx,
            ),
            update_url: mk(
                s.update.url.clone().unwrap_or_default(),
                "https://".into(),
                window,
                cx,
            ),
            message: None,
        }
    }

    fn apply(&mut self, change: Change, cx: &mut Context<Self>) {
        self.message = match super::commit(cx, change) {
            Ok(true) => Some((false, t!("settings.restart_needed").to_string())),
            Ok(false) => None,
            Err(e) => Some((true, e)),
        };
        cx.notify();
    }
}

fn muted() -> gpui_color::C {
    rgba(0xF4F1E8A0).into()
}

mod gpui_color {
    pub use crate::ui::Hsla as C;
}

fn row(label: String, control: impl IntoElement) -> impl IntoElement {
    h_flex()
        .justify_between()
        .items_center()
        .gap(px(16.0))
        .py(px(6.0))
        .child(div().flex_1().child(label))
        .child(control)
}

fn heading(text: String) -> impl IntoElement {
    div()
        .mt(px(8.0))
        .mb(px(4.0))
        .text_size(px(18.0))
        .child(text)
}

fn note(text: String) -> impl IntoElement {
    div().text_size(px(12.0)).text_color(muted()).child(text)
}

fn toggle(
    weak: &WeakEntity<SettingsView>,
    id: &'static str,
    label_key: &str,
    on: bool,
    make: fn(bool) -> Change,
) -> impl IntoElement {
    let weak = weak.clone();
    row(
        tr(label_key),
        Switch::new(id).checked(on).on_click(move |v, _, cx| {
            let v = *v;
            weak.update(cx, |t, cx| t.apply(make(v), cx)).ok();
        }),
    )
}

fn placeholder_section(key: &str) -> AnyElement {
    note(tr(key)).into_any_element()
}

impl SettingsView {
    fn section_title(id: &str) -> String {
        tr(&format!("settings.section.{id}"))
    }

    fn body(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(c) = super::contributed(cx, id) {
            return c.render(window, cx);
        }
        let weak = cx.weak_entity();
        let s = super::current(cx).unwrap_or_default();
        match id {
            "connection" => {
                let w = weak.clone();
                let url = self.url.clone();
                v_flex()
                    .gap(px(6.0))
                    .child(heading(Self::section_title("connection")))
                    .child(row(
                        t!("settings.connection.url").to_string(),
                        div().w(px(320.0)).child(Input::new(&self.url)),
                    ))
                    .child(
                        Button::new("set-url")
                            .label(t!("settings.apply").to_string())
                            .on_click(move |_, _, cx| {
                                let v = url.read(cx).value().to_string();
                                w.update(cx, |t, cx| t.apply(Change::PandoUrl(Some(v)), cx))
                                    .ok();
                            }),
                    )
                    .child(note(t!("settings.connection.restart").to_string()))
                    .child(note(t!("settings.connection.keychain").to_string()))
                    .into_any_element()
            }
            "voice" | "audio" => v_flex()
                .child(heading(Self::section_title(id)))
                .child(placeholder_section(&format!("settings.{id}.placeholder")))
                .into_any_element(),
            "shortcuts" => {
                let w = weak.clone();
                let sc = self.shortcut.clone();
                let compositor = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
                let compositor = if std::env::var_os("SWAYSOCK").is_some() {
                    "sway".to_owned()
                } else {
                    compositor
                };
                let lines = portal::fallback_lines(&compositor, &s.shell.ptt_shortcut);
                let status = ["global-hotkey", "portal"]
                    .iter()
                    .filter_map(|n| runtime::backend_status(cx, n).map(|v| format!("{n}: {v}")))
                    .collect::<Vec<_>>()
                    .join("\n");
                v_flex()
                    .gap(px(6.0))
                    .child(heading(Self::section_title("shortcuts")))
                    .child(row(
                        t!("settings.shortcuts.ptt").to_string(),
                        div().w(px(240.0)).child(Input::new(&self.shortcut)),
                    ))
                    .child(
                        Button::new("set-shortcut")
                            .label(t!("settings.apply").to_string())
                            .on_click(move |_, _, cx| {
                                let v = sc.read(cx).value().to_string();
                                w.update(cx, |t, cx| t.apply(Change::PttShortcut(v), cx))
                                    .ok();
                            }),
                    )
                    .child(heading(t!("settings.shortcuts.status").to_string()))
                    .child(note(if status.is_empty() {
                        t!("settings.shortcuts.no_status").to_string()
                    } else {
                        status
                    }))
                    .child(heading(t!("settings.shortcuts.cli").to_string()))
                    .child(note(t!("settings.shortcuts.cli_help").to_string()))
                    .child(
                        v_flex()
                            .p(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(palette::BOSQUE_DEEP))
                            .font_family(crate::fonts::FONT_MONO)
                            .text_size(px(12.0))
                            .children(lines.into_iter().map(|l| div().child(l))),
                    )
                    .child(note(t!("settings.shortcuts.keymap").to_string()))
                    .into_any_element()
            }
            "appearance" => {
                let lang_buttons =
                    std::iter::once((None::<&str>, t!("settings.appearance.auto").to_string()))
                        .chain(
                            crate::i18n::LANGUAGES
                                .iter()
                                .map(|(tag, name)| (Some(*tag), (*name).to_owned())),
                        )
                        .enumerate()
                        .map(|(i, (tag, name))| {
                            let w = weak.clone();
                            let selected = s.language.as_deref() == tag;
                            Button::new(("lang", i))
                                .label(name)
                                .selected(selected)
                                .on_click(move |_, _, cx| {
                                    w.update(cx, |t, cx| {
                                        t.apply(Change::Language(tag.map(str::to_owned)), cx)
                                    })
                                    .ok();
                                })
                        })
                        .collect::<Vec<_>>();
                v_flex()
                    .gap(px(6.0))
                    .child(heading(Self::section_title("appearance")))
                    .child(row(
                        t!("settings.appearance.language").to_string(),
                        h_flex().gap_1().children(lang_buttons),
                    ))
                    .child(toggle(
                        &weak,
                        "reduce-motion",
                        "settings.appearance.reduce_motion",
                        s.reduce_motion,
                        Change::ReduceMotion,
                    ))
                    .child(note(t!("settings.appearance.reduce_motion_os").to_string()))
                    .child(note(t!("settings.appearance.layout_pending").to_string()))
                    .into_any_element()
            }
            "system" => {
                let target_on = directories::BaseDirs::new().is_some_and(|d| {
                    let cfg = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from);
                    autostart::is_enabled(&autostart::target_for_os(d.home_dir(), cfg.as_deref()))
                });
                let update = match crate::update::availability(&s.update) {
                    crate::update::Availability::Enabled => {
                        t!("settings.system.update_on").to_string()
                    }
                    crate::update::Availability::Disabled(why) => {
                        t!("settings.system.update_off", why = format!("{why:?}")).to_string()
                    }
                };
                let w = weak.clone();
                let uurl = self.update_url.clone();
                let wf = weak.clone();
                v_flex()
                    .gap(px(2.0))
                    .child(heading(Self::section_title("system")))
                    .child(toggle(
                        &weak,
                        "autostart",
                        "settings.system.autostart",
                        target_on || s.shell.autostart,
                        Change::Autostart,
                    ))
                    .child(toggle(
                        &weak,
                        "keep-running",
                        "settings.system.keep_running",
                        s.shell.keep_running,
                        Change::KeepRunning,
                    ))
                    .child(note(if runtime::tray_ready(cx) {
                        t!("settings.system.tray_ready").to_string()
                    } else {
                        t!("settings.system.tray_missing").to_string()
                    }))
                    .child(toggle(
                        &weak,
                        "start-min",
                        "settings.system.start_minimized",
                        s.shell.start_minimized,
                        Change::StartMinimized,
                    ))
                    .child(toggle(
                        &weak,
                        "mini",
                        "settings.system.mini",
                        s.shell.mini_enabled,
                        Change::MiniEnabled,
                    ))
                    .child(toggle(
                        &weak,
                        "mini-hide",
                        "settings.system.mini_auto_hide",
                        s.shell.mini_auto_hide,
                        Change::MiniAutoHide,
                    ))
                    .child(toggle(
                        &weak,
                        "notify",
                        "settings.system.notifications",
                        s.shell.system_notifications,
                        Change::SystemNotifications,
                    ))
                    .child(heading(t!("settings.system.updates").to_string()))
                    .child(toggle(
                        &weak,
                        "update",
                        "settings.system.update_enabled",
                        s.update.enabled,
                        Change::UpdateEnabled,
                    ))
                    .child(row(
                        t!("settings.system.update_url").to_string(),
                        div().w(px(320.0)).child(Input::new(&self.update_url)),
                    ))
                    .child(
                        Button::new("set-update-url")
                            .label(t!("settings.apply").to_string())
                            .on_click(move |_, _, cx| {
                                let v = uurl.read(cx).value().to_string();
                                w.update(cx, |t, cx| t.apply(Change::UpdateUrl(Some(v)), cx))
                                    .ok();
                            }),
                    )
                    .child(note(update))
                    .child(div().h(px(8.0)))
                    .child(
                        Button::new("reopen-first-run")
                            .label(t!("settings.system.first_run").to_string())
                            .on_click(move |_, _, cx| {
                                wf.update(cx, |_, cx| crate::firstrun::open(cx)).ok();
                            }),
                    )
                    .into_any_element()
            }
            _ => {
                let url = s
                    .pando_url
                    .clone()
                    .unwrap_or_else(|| "http://127.0.0.1:8080".into());
                let logs = directories::ProjectDirs::from("dev", "Pando", "pando-assistant")
                    .map(|d| d.cache_dir().join("logs"));
                v_flex()
                    .gap(px(6.0))
                    .child(heading(Self::section_title("advanced")))
                    .child(
                        Button::new("open-webui")
                            .label(t!("settings.advanced.webui").to_string())
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
                    .children(logs.map(|p| {
                        Button::new("open-logs")
                            .label(t!("settings.advanced.logs").to_string())
                            .on_click(move |_, _, cx| {
                                cx.open_url(&format!("file://{}", p.display()))
                            })
                    }))
                    .child(note(t!("settings.advanced.decisions").to_string()))
                    .into_any_element()
            }
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let extra = super::extra_sections(cx);
        let mut ids: Vec<String> = SECTIONS.iter().map(|s| (*s).to_owned()).collect();
        ids.extend(extra.iter().map(|s| s.id().to_owned()));
        let active = self.active.min(ids.len() - 1);
        let nav = ids.iter().enumerate().map(|(i, id)| {
            let w = weak.clone();
            let title = extra
                .iter()
                .find(|s| s.id() == id)
                .map_or_else(|| Self::section_title(id), |s| s.title());
            Button::new(("nav", i))
                .ghost()
                .label(title)
                .selected(i == active)
                .on_click(move |_, _, cx| {
                    w.update(cx, |t, cx| {
                        t.active = i;
                        t.message = None;
                        cx.notify();
                    })
                    .ok();
                })
        });
        let body = self.body(&ids[active].clone(), window, cx);
        let message = self.message.clone();
        h_flex()
            .id("settings")
            .size_full()
            .bg(rgb(palette::BOSQUE))
            .text_color(rgb(palette::MARFIL))
            .child(
                v_flex()
                    .w(px(200.0))
                    .h_full()
                    .p(px(12.0))
                    .gap(px(2.0))
                    .border_r_1()
                    .border_color(rgba(0xF4F1E81F))
                    .children(nav),
            )
            .child(
                v_flex()
                    .id("settings-body")
                    .flex_1()
                    .h_full()
                    .overflow_y_scroll()
                    .p(px(24.0))
                    .gap(px(8.0))
                    .child(body)
                    .children(message.map(|(error, text)| {
                        div()
                            .mt(px(8.0))
                            .text_size(px(12.0))
                            .text_color(if error {
                                rgb(palette::BERMELLON)
                            } else {
                                rgb(palette::ALAMO)
                            })
                            .child(text)
                    })),
            )
    }
}
