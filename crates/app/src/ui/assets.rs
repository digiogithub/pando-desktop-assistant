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

/// The bytes of an embedded brand SVG, exactly as shipped in `design-system/brand/`.
#[must_use]
pub fn brand_svg(path: &str) -> Option<&'static [u8]> {
    BRAND.iter().find(|(p, _)| *p == path).map(|(_, b)| *b)
}

/// Intrinsic width given to the wordmarks. Their `viewBox` is 4132 units wide and GPUI rasterizes
/// an `img()` SVG at twice its intrinsic size (8264 px, past the 8192 px texture cap), so the
/// shipped files get a modest `width`/`height` at load time.
const WORDMARK_WIDTH: f32 = 413.2;

/// The bytes served for `path`: wordmarks carry an explicit intrinsic size, the rest is verbatim.
fn served(path: &str, bytes: &'static [u8]) -> Cow<'static, [u8]> {
    const OPEN: &[u8] = b"<svg ";
    let wordmark = path == WORDMARK_ON_DARK || path == WORDMARK_ON_LIGHT;
    if !wordmark || !bytes.starts_with(OPEN) {
        return Cow::Borrowed(bytes);
    }
    let height = WORDMARK_WIDTH * 1080.0 / 4132.0;
    let mut out = format!("<svg width=\"{WORDMARK_WIDTH}\" height=\"{height:.1}\" ").into_bytes();
    out.extend_from_slice(&bytes[OPEN.len()..]);
    Cow::Owned(out)
}

/// Brand SVGs first, then the kit's default icons.
#[derive(Debug, Clone, Copy, Default)]
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = brand_svg(path) {
            return Ok(Some(served(path, bytes)));
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
    fn wordmarks_are_served_with_a_small_intrinsic_size() {
        let bytes = AppAssets
            .load(WORDMARK_ON_DARK)
            .expect("load")
            .expect("embedded");
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.starts_with("<svg width=\"413.2\" height=\"108.0\" "),
            "{}",
            &text[..80]
        );
        // Icons are served verbatim.
        let icon = AppAssets.load(APP_ICON).expect("load").expect("embedded");
        assert_eq!(&*icon, brand_svg(APP_ICON).expect("icon"));
    }

    #[test]
    fn kit_icons_still_resolve_and_unknown_paths_do_not() {
        assert!(AppAssets.load("icons/check.svg").expect("load").is_some());
        assert!(AppAssets.load("brand/nope.svg").ok().flatten().is_none());
        assert!(icon_svg("icons/check.svg").is_some());
    }
}
