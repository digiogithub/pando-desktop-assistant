//! Microphone permission banner and device selector with a test level meter (PDA-T-0086).

use rust_i18n::t;

use super::controller::VoiceController;
use super::devices::{self, DeviceChoice, MicProblem};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::theme::palette;
use crate::ui::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, div, h_flex, px, rgb, rgba, v_flex,
};

fn marfil(c: palette::Rgba) -> Hsla {
    rgba(c.as_u32()).into()
}

impl VoiceController {
    /// The permission sheet shown while the OS refuses the microphone.
    pub(super) fn permission_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.dock.mic_problem != Some(MicProblem::PermissionDenied) {
            return None;
        }
        let link = devices::privacy_settings_url();
        let action = match link {
            Some(url) => Button::new("voice-open-privacy")
                .primary()
                .label(t!("voice.permission.open_settings").to_string())
                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(url)))
                .into_any_element(),
            None => div()
                .text_size(px(11.0))
                .child(t!("voice.permission.no_link").to_string())
                .into_any_element(),
        };
        Some(
            v_flex()
                .id("voice-permission")
                .gap(px(6.0))
                .p(px(12.0))
                .m(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(rgb(palette::BERMELLON))
                .child(
                    div()
                        .text_size(px(14.0))
                        .child(t!("voice.permission.title").to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(marfil(palette::MARFIL_ALPHA_62))
                        .child(t!("voice.permission.body").to_string()),
                )
                .child(action)
                .into_any_element(),
        )
    }

    /// The settings panel: enable switch, device lists and the meter.
    pub(super) fn settings_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.settings_open_flag() {
            return None;
        }
        let selected_in = self.config().input_device.clone();
        let selected_out = self.config().output_device.clone();
        let level = self
            .service()
            .and_then(|s| s.mic_level())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let list = |prefix: &'static str,
                    devices: &[DeviceChoice],
                    current: &Option<String>,
                    input: bool,
                    cx: &mut Context<Self>| {
            v_flex()
                .gap(px(4.0))
                .children(devices.iter().enumerate().map(|(i, d)| {
                    let id = d.id.clone();
                    let active = &d.id == current;
                    let button =
                        Button::new((prefix, i))
                            .label(d.name.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if input {
                                    this.select_input(id.clone(), cx);
                                } else {
                                    this.select_output(id.clone(), cx);
                                }
                            }));
                    if active {
                        button.primary()
                    } else {
                        button.ghost()
                    }
                }))
        };
        let report = self.stats.report();
        let latency = v_flex()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .child(t!("voice.debug.title").to_string()),
            )
            .children(if report.is_empty() {
                vec![
                    div()
                        .text_size(px(11.0))
                        .text_color(marfil(palette::MARFIL_ALPHA_45))
                        .child(t!("voice.debug.empty").to_string()),
                ]
            } else {
                report
                    .into_iter()
                    .map(|line| {
                        div()
                            .font_family(crate::fonts::FONT_MONO)
                            .text_size(px(11.0))
                            .child(line)
                    })
                    .collect()
            });
        let enabled = self.config().enabled;
        let toggle = Button::new("voice-enabled")
            .label(format!(
                "{}: {}",
                t!("voice.settings.enabled"),
                if enabled { "✓" } else { "✗" }
            ))
            .on_click(cx.listener(move |this, _, _, cx| this.set_enabled(!enabled, cx)));
        Some(
            v_flex()
                .id("voice-settings")
                .gap(px(10.0))
                .p(px(12.0))
                .border_t_1()
                .border_color(marfil(palette::MARFIL_ALPHA_12))
                .child(
                    div()
                        .text_size(px(14.0))
                        .child(t!("voice.settings.title").to_string()),
                )
                .child(toggle)
                .child(
                    div()
                        .text_size(px(11.0))
                        .child(t!("voice.settings.input").to_string()),
                )
                .child(list(
                    "voice-in",
                    &self.input_choices(),
                    &selected_in,
                    true,
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .child(t!("voice.settings.output").to_string()),
                )
                .child(list(
                    "voice-out",
                    &self.output_choices(),
                    &selected_out,
                    false,
                    cx,
                ))
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .items_center()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .child(t!("voice.settings.meter").to_string()),
                        )
                        .child(
                            div()
                                .h(px(4.0))
                                .w(px(240.0))
                                .bg(marfil(palette::MARFIL_ALPHA_12))
                                .child(
                                    div()
                                        .h(px(4.0))
                                        .w(crate::ui::text_edit::relative(level))
                                        .bg(rgb(palette::ALAMO)),
                                ),
                        ),
                )
                .child(latency)
                .into_any_element(),
        )
    }
}
