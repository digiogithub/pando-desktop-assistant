//! Installs the generated Pando kit theme (`assets/themes/pando.json`, produced by
//! `cargo xtask tokens` from `design-system/tokens.json`) into GPUI Kit.
//!
//! The assistant is dark-only (Bosque background); the kit's light slot is pointed at the same
//! theme registration order but never selected.

use gpui_kit::component::theme::{Theme, ThemeMode, ThemeRegistry};

use super::App;

/// The generated theme set, embedded in the binary.
const PANDO_THEMES: &str = include_str!("../../assets/themes/pando.json");

/// Name of the dark theme inside [`PANDO_THEMES`].
pub const DARK_THEME: &str = "Pando Dark";

/// Registers the Pando themes and activates the dark one. Call after [`super::init`].
pub fn install(cx: &mut App) {
    if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(PANDO_THEMES) {
        tracing::warn!("cannot load the Pando theme, keeping the kit default: {err}");
        return;
    }
    let dark = ThemeRegistry::global(cx).themes().get(DARK_THEME).cloned();
    match dark {
        Some(dark) if dark.mode == ThemeMode::Dark => {
            Theme::global_mut(cx).dark_theme = dark;
            Theme::change(ThemeMode::Dark, None, cx);
        }
        _ => tracing::warn!("theme `{DARK_THEME}` missing or not dark, keeping the kit default"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};
    use crate::ui::theme::palette;
    use crate::ui::{ActiveTheme as _, rgb};

    #[gpui_test]
    fn pando_dark_is_applied_to_the_kit_theme(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            install(cx);
            let theme = cx.theme();
            assert!(theme.is_dark());
            assert_eq!(theme.colors.background, rgb(palette::BOSQUE).into());
            assert_eq!(theme.colors.foreground, rgb(palette::MARFIL).into());
            assert_eq!(theme.colors.primary, rgb(palette::ALAMO).into());
        });
    }
}
