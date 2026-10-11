//! Embedded design-system fonts (`design-system/README.md` §1).
//!
//! Static TTFs of Space Grotesk (display, 400/500/700), JetBrains Mono (labels and streams,
//! 300/400) and Zen Kaku Gothic New (UI copy, 300) ship inside the binary and are registered
//! with the GPUI text system before the first window. All three are SIL OFL 1.1; the licence
//! texts live in `assets/fonts/OFL-*.txt` and are shipped as resources (see `NOTICE`).
//!
//! Space Grotesk has no static SemiBold, so the 600 weight of the tokens resolves to Bold.
//! Pattern from Bitácora's `fonts.rs` (MIT).

use std::borrow::Cow;

use crate::ui::{App, TextSystem};

/// Display family (wordmark, state label, agent names).
pub const FONT_DISPLAY: &str = "Space Grotesk";
/// Monospace family (labels, eyebrows, streams, keyboard hints).
pub const FONT_MONO: &str = "JetBrains Mono";
/// UI copy family; light weight by default.
pub const FONT_BODY: &str = "Zen Kaku Gothic New";

/// The embedded static TTFs.
const EMBEDDED_FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/SpaceGrotesk-Regular.ttf"),
    include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf"),
    include_bytes!("../assets/fonts/SpaceGrotesk-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Light.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/ZenKakuGothicNew-Light.ttf"),
];

/// Registers the embedded fonts. A failure is logged, not fatal: GPUI falls back to system
/// fonts.
pub fn register(cx: &mut App) {
    add(cx.text_system());
}

fn add(system: &TextSystem) {
    let fonts = EMBEDDED_FONTS.iter().map(|b| Cow::Borrowed(*b)).collect();
    if let Err(err) = system.add_fonts(fonts) {
        tracing::warn!("cannot register the embedded fonts: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::platform_text_system;
    use crate::ui::text_edit::{Font, FontWeight};

    // Needs the real platform font stack on the test thread. macOS creates its platform on the
    // main thread only and Windows DirectWrite does not list fonts added from memory, so it
    // only runs against the Linux (cosmic-text) stack.
    #[test]
    #[cfg_attr(
        not(target_os = "linux"),
        ignore = "needs the Linux cosmic-text font stack"
    )]
    fn embedded_families_resolve() {
        let system = platform_text_system();
        add(&system);
        let names = system.all_font_names();
        for family in [FONT_DISPLAY, FONT_MONO, FONT_BODY] {
            assert!(
                names.iter().any(|n| n == family),
                "{family} missing: {names:?}"
            );
        }
        // Every embedded weight resolves without falling back (resolve_font panics when
        // neither the font nor any fallback loads).
        for (family, weight) in [
            (FONT_DISPLAY, FontWeight::NORMAL),
            (FONT_DISPLAY, FontWeight::MEDIUM),
            (FONT_DISPLAY, FontWeight::SEMIBOLD),
            (FONT_MONO, FontWeight::LIGHT),
            (FONT_MONO, FontWeight::NORMAL),
            (FONT_BODY, FontWeight::LIGHT),
        ] {
            let font = Font {
                family: family.into(),
                weight,
                ..Font::default()
            };
            system.resolve_font(&font);
        }
    }

    #[test]
    fn every_font_has_its_licence_file() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
        for licence in [
            "OFL-SpaceGrotesk.txt",
            "OFL-JetBrainsMono.txt",
            "OFL-ZenKakuGothicNew.txt",
        ] {
            let text = std::fs::read_to_string(dir.join(licence)).expect("licence file");
            assert!(text.contains("SIL OPEN FONT LICENSE"), "{licence}");
        }
    }
}
