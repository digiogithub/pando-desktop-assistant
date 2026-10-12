//! Colours and small element helpers shared by the chat views (palette: `ui::theme::palette`).

use crate::ui::theme::palette;
use crate::ui::{Hsla, rgb, rgba};

/// The chat's colour set, derived from the brand palette.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    /// Window background.
    pub bg: Hsla,
    /// Recessed surfaces (code, inputs).
    pub deep: Hsla,
    /// Raised surfaces (cards).
    pub card: Hsla,
    /// Primary text.
    pub text: Hsla,
    /// Secondary text.
    pub text2: Hsla,
    /// Tertiary text.
    pub text3: Hsla,
    /// Hairline.
    pub line: Hsla,
    /// Stronger hairline.
    pub line2: Hsla,
    /// Hover wash.
    pub hover: Hsla,
    /// Accent (gold).
    pub accent: Hsla,
    /// Text on the accent.
    pub on_accent: Hsla,
    /// Danger.
    pub danger: Hsla,
    /// Informational blue.
    pub info: Hsla,
    /// Added lines: text.
    pub add_fg: Hsla,
    /// Added lines: background.
    pub add_bg: Hsla,
    /// Removed lines: text.
    pub del_fg: Hsla,
    /// Removed lines: background.
    pub del_bg: Hsla,
}

/// An RGBA palette constant as a colour.
#[must_use]
pub fn alpha(c: palette::Rgba) -> Hsla {
    rgba(c.as_u32()).into()
}

fn a(rgb_hex: u32, alpha: u8) -> Hsla {
    rgba((rgb_hex << 8) | u32::from(alpha)).into()
}

impl Colors {
    /// The colours.
    #[must_use]
    pub fn get() -> Self {
        Self {
            bg: rgb(palette::BOSQUE).into(),
            deep: rgb(palette::BOSQUE_DEEP).into(),
            card: a(palette::MARFIL, 0x0A),
            text: rgb(palette::MARFIL).into(),
            text2: alpha(palette::MARFIL_ALPHA_62),
            text3: alpha(palette::MARFIL_ALPHA_45),
            line: alpha(palette::MARFIL_ALPHA_12),
            line2: alpha(palette::MARFIL_ALPHA_22),
            hover: alpha(palette::MARFIL_ALPHA_07),
            accent: rgb(palette::ALAMO).into(),
            on_accent: rgb(palette::BOSQUE_DEEP).into(),
            danger: rgb(palette::BERMELLON).into(),
            info: rgb(palette::CLAUDE_BLUE).into(),
            add_fg: rgb(0x9BD7A5).into(),
            add_bg: a(0x4CAF68, 0x26),
            del_fg: rgb(0xF0A096).into(),
            del_bg: a(palette::BERMELLON, 0x26),
        }
    }
}

use std::time::Duration;

use crate::ui::{
    Animation, AnimationExt as _, AnyElement, IntoElement, SharedString, Styled as _, div,
    pulsating_between, px,
};

/// A small dot that breathes while something is running.
#[must_use]
pub fn pulse_dot(id: impl Into<SharedString>, color: Hsla, size: f32) -> AnyElement {
    use crate::ui::ElementId;
    let id: SharedString = id.into();
    div()
        .size(px(size))
        .rounded_full()
        .bg(color)
        .with_animation(
            ElementId::from(id),
            Animation::new(Duration::from_millis(1400))
                .repeat()
                .with_easing(pulsating_between(0.3, 1.0)),
            |d, v| d.opacity(v),
        )
        .into_any_element()
}
