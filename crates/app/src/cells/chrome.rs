//! Orb chrome (PDA-T-0117): the status label, hint, mic button and legend under the canvas, and
//! the info panel beside it ("prueba un ejemplo", "voz -> decisión -> acción", "célula"),
//! following the "Pando Desktop · organismo vivo" artboard (design-system README §5).

use organism::{BodyState, tokens};
use rust_i18n::t;

use crate::organism_view::{demo::state_name, tr};
use crate::shell::ipc::{Command, PttAction};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::theme::palette;
use crate::ui::{
    Hsla, IntoElement, ParentElement as _, Selectable as _, Styled as _, div, h_flex, px, rgb,
    rgba, v_flex,
};

/// Widest the side panel may be (design system).
pub const SIDE_PANEL_MAX: f32 = 500.0;
/// Width used by the info panel and the cell card.
pub const SIDE_PANEL_WIDTH: f32 = 340.0;
const _: () = assert!(SIDE_PANEL_WIDTH <= SIDE_PANEL_MAX);

fn marfil(c: palette::Rgba) -> Hsla {
    rgba(c.as_u32()).into()
}

fn state_id(s: BodyState) -> &'static str {
    match s {
        BodyState::Idle => "idle",
        BodyState::Listening => "listening",
        BodyState::Thinking => "thinking",
        BodyState::Speaking => "speaking",
        BodyState::Delegating => "delegating",
    }
}

/// Localized hint under the status label.
#[must_use]
pub fn hint_for(s: BodyState) -> String {
    tr(&format!("chrome.hint.{}", state_id(s)))
}

/// The three example phrases.
#[must_use]
pub fn examples() -> Vec<String> {
    (1..=3)
        .map(|i| tr(&format!("chrome.examples.p{i}")))
        .collect()
}

/// Status label (Space Grotesk 500, 19 px) and hint (13 px, Marfil 62 %).
pub fn status(s: BodyState) -> impl IntoElement {
    v_flex()
        .items_center()
        .gap(px(2.0))
        .child(
            div()
                .font_family(crate::fonts::FONT_DISPLAY)
                .font_weight(crate::ui::text_edit::FontWeight::MEDIUM)
                .text_size(px(19.0))
                .child(state_name(s)),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(marfil(palette::MARFIL_ALPHA_62))
                .child(hint_for(s)),
        )
}

/// Pill mic button; pressing it toggles hold-to-talk through the shell.
pub fn mic_button(listening: bool) -> Button {
    Button::new("orb-mic")
        .primary()
        .selected(listening)
        .label(
            if listening {
                t!("chrome.mic.stop")
            } else {
                t!("chrome.mic.start")
            }
            .to_string(),
        )
        .on_click(|_, _, cx| {
            crate::shell::runtime::send_command(
                cx,
                Command::Ptt {
                    action: PttAction::Toggle,
                },
            );
        })
}

fn dot(c: tokens::Color, fill: bool) -> impl IntoElement {
    let col: Hsla = rgba((c.rgb << 8) | u32::from(c.a)).into();
    let d = div()
        .size(px(9.0))
        .rounded_full()
        .border_2()
        .border_color(col);
    if fill { d.bg(col) } else { d }
}

/// Legend of engines and statuses.
pub fn legend() -> impl IntoElement {
    let engines = tokens::ENGINES.iter().map(|e| {
        h_flex()
            .gap(px(5.0))
            .items_center()
            .child(dot(e.nucleus, e.fill))
            .child(tr(&format!("organism.engine.{}", e.id)))
    });
    let statuses = tokens::STATUSES.iter().map(|s| {
        h_flex()
            .gap(px(4.0))
            .items_center()
            .child(
                div()
                    .min_w(px(10.0))
                    .flex_shrink_0()
                    .text_color(marfil(palette::MARFIL_ALPHA_62))
                    .child(s.badge.unwrap_or("·")),
            )
            .child(tr(&format!("organism.status.{}", s.id)))
    });
    v_flex()
        .gap(px(4.0))
        .text_size(px(11.0))
        .font_family(crate::fonts::FONT_MONO)
        .text_color(marfil(palette::MARFIL_ALPHA_62))
        .child(
            h_flex()
                .gap(px(12.0))
                .flex_wrap()
                .justify_center()
                .children(engines),
        )
        .child(
            h_flex()
                .gap(px(12.0))
                .flex_wrap()
                .justify_center()
                .children(statuses),
        )
}

fn card(title: String, body: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap(px(6.0))
        .p(px(12.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(marfil(palette::MARFIL_ALPHA_12))
        .child(
            div()
                .text_size(px(11.0))
                .font_family(crate::fonts::FONT_MONO)
                .text_color(rgb(palette::ALAMO))
                .child(title),
        )
        .child(body)
}

/// The info side panel shown when no cell is selected.
pub fn info_panel(last_transcript: Option<&str>) -> impl IntoElement {
    let examples = v_flex()
        .gap(px(4.0))
        .children(examples().into_iter().map(|p| div().child(p)));
    let flow = v_flex()
        .gap(px(4.0))
        .child(div().child(t!("chrome.flow.body").to_string()))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(marfil(palette::MARFIL_ALPHA_62))
                .child(
                    last_transcript
                        .map(str::to_owned)
                        .unwrap_or_else(|| t!("chrome.flow.empty").to_string()),
                ),
        );
    v_flex()
        .w(px(SIDE_PANEL_WIDTH.min(SIDE_PANEL_MAX)))
        .h_full()
        .flex_shrink_0()
        .p(px(16.0))
        .gap(px(12.0))
        .border_l_1()
        .border_color(marfil(palette::MARFIL_ALPHA_12))
        .text_size(px(13.0))
        .child(card(t!("chrome.examples.title").to_string(), examples))
        .child(card(t!("chrome.flow.title").to_string(), flow))
        .child(card(
            t!("chrome.cell.title").to_string(),
            div().child(t!("chrome.cell.body").to_string()),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_a_hint_and_examples_exist() {
        rust_i18n::set_locale("es");
        for s in BodyState::ALL {
            let h = hint_for(s);
            assert!(!h.starts_with("chrome."), "{h}");
        }
        assert_eq!(examples().len(), 3);
    }
}
