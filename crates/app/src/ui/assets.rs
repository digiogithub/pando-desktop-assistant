//! Asset source of the app: the brand SVGs of `design-system/brand/` embedded in the binary,
//! layered over the GPUI Kit icon catalog.
//!
//! Brand SVGs are two-tone, so they are painted with `img()` (`ui::img`), never with
//! `paint_svg`, which tints everything with a single colour. Paths are `brand/<file>.svg`.

use std::borrow::Cow;

use gpui_kit::assets::{AllAssets, Assets};
use gpui_kit::gpui::{AssetSource, SharedString};

/// Wordmark (symbol + "pando") for dark surfaces: Marfil ink, Álamo accents.
pub const WORDMARK_ON_DARK: &str = "brand/pando-logo-light.svg";
/// Wordmark for light surfaces.
pub const WORDMARK_ON_LIGHT: &str = "brand/pando-logo-dark.svg";
/// Symbol only, for dark surfaces.
pub const MARK_ON_DARK: &str = "brand/pando-mark-light.svg";
/// Symbol only, for light surfaces.
pub const MARK_ON_LIGHT: &str = "brand/pando-mark-dark.svg";
/// Application icon (tile with the symbol).
pub const APP_ICON: &str = "brand/pando-icon.svg";
/// Mesnada (main agent) mark for dark surfaces.
pub const MESNADA_MARK_ON_DARK: &str = "brand/mesnada-mark-light.svg";
/// Mesnada mark for light surfaces.
pub const MESNADA_MARK_ON_LIGHT: &str = "brand/mesnada-mark-dark.svg";
/// Mesnada icon (tile).
pub const MESNADA_ICON: &str = "brand/mesnada-icon.svg";

/// Every embedded brand asset: `(path, bytes)`.
const BRAND: &[(&str, &[u8])] = &[
    (
        WORDMARK_ON_DARK,
        include_bytes!("../../../../design-system/brand/pando-logo-light.svg"),
    ),
    (
        WORDMARK_ON_LIGHT,
        include_bytes!("../../../../design-system/brand/pando-logo-dark.svg"),
    ),
    (
        MARK_ON_DARK,
        include_bytes!("../../../../design-system/brand/pando-mark-light.svg"),
    ),
    (
        MARK_ON_LIGHT,
        include_bytes!("../../../../design-system/brand/pando-mark-dark.svg"),
    ),
    (
        APP_ICON,
        include_bytes!("../../../../design-system/brand/pando-icon.svg"),
    ),
    (
        MESNADA_MARK_ON_DARK,
        include_bytes!("../../../../design-system/brand/mesnada-mark-light.svg"),
    ),
    (
        MESNADA_MARK_ON_LIGHT,
        include_bytes!("../../../../design-system/brand/mesnada-mark-dark.svg"),
    ),
    (
        MESNADA_ICON,
        include_bytes!("../../../../design-system/brand/mesnada-icon.svg"),
    ),
];

/// The bytes of an embedded brand SVG.
#[must_use]
pub fn brand_svg(path: &str) -> Option<&'static [u8]> {
    BRAND.iter().find(|(p, _)| *p == path).map(|(_, b)| *b)
}

/// Brand SVGs first, then the kit's default icons.
#[derive(Debug, Clone, Copy, Default)]
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = brand_svg(path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        if path.trim_end_matches('/') == "brand" {
            return Ok(BRAND.iter().map(|(p, _)| SharedString::from(*p)).collect());
        }
        Assets.list(path)
    }
}

/// The bytes of an embedded Lucide icon (`icons/<lucide-name>.svg`, the full catalog).
#[must_use]
pub fn icon_svg(path: &str) -> Option<Vec<u8>> {
    AllAssets.load(path).ok().flatten().map(Cow::into_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_svgs_load_and_are_svg() {
        for path in [
            WORDMARK_ON_DARK,
            WORDMARK_ON_LIGHT,
            MARK_ON_DARK,
            MARK_ON_LIGHT,
            APP_ICON,
            MESNADA_MARK_ON_DARK,
            MESNADA_MARK_ON_LIGHT,
            MESNADA_ICON,
        ] {
            let bytes = AppAssets.load(path).expect("load").expect("embedded");
            assert!(bytes.starts_with(b"<svg"), "{path} is not an SVG");
        }
    }

    #[test]
    fn two_tone_wordmark_keeps_both_colours() {
        let svg =
            String::from_utf8_lossy(brand_svg(WORDMARK_ON_DARK).expect("wordmark")).into_owned();
        // Marfil text and Álamo accents: tinting it with one colour would lose one of them.
        assert!(svg.contains("#F4F1E8") && svg.contains("#E9B949"));
    }

    #[test]
    fn kit_icons_still_resolve_and_unknown_paths_do_not() {
        assert!(AppAssets.load("icons/check.svg").expect("load").is_some());
        assert!(AppAssets.load("brand/nope.svg").ok().flatten().is_none());
        assert!(icon_svg("icons/check.svg").is_some());
    }
}
