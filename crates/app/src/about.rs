//! The About window (PDA-T-0151): version, licence, third-party notices, model attributions and
//! the GPL source offer for `pando-tts`.
//!
//! `packaging/NOTICE` and `packaging/SOURCE-OFFER.md` are embedded; the generated crate inventory
//! (`cargo xtask notice` -> `THIRD-PARTY-pando-assistant.md`) is shipped next to the binary by the
//! packagers and shown when found.

use std::path::{Path, PathBuf};

use rust_i18n::t;

use crate::ui::theme::palette;
use crate::ui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb, rgba, v_flex,
};

/// Embedded hand-maintained notice (non-crate components, fonts, models, runtimes).
pub const NOTICE: &str = include_str!("../../../packaging/NOTICE");
/// Embedded source offer for the GPL-3 `pando-tts` worker.
pub const SOURCE_OFFER: &str = include_str!("../../../packaging/SOURCE-OFFER.md");
/// File name of the generated inventory.
pub const INVENTORY_FILE: &str = "THIRD-PARTY-pando-assistant.md";

/// Places where packagers put the generated inventory, best first.
#[must_use]
pub fn inventory_candidates(exe: Option<&Path>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(dir) = exe.and_then(Path::parent) {
        v.push(dir.join("legal").join(INVENTORY_FILE));
        v.push(
            dir.join("../share/doc/pando-assistant")
                .join(INVENTORY_FILE),
        );
        v.push(dir.join("../Resources/legal").join(INVENTORY_FILE));
    }
    v.push(PathBuf::from("/usr/share/doc/pando-assistant").join(INVENTORY_FILE));
    v
}

/// The generated inventory, if installed.
#[must_use]
pub fn read_inventory() -> Option<String> {
    let exe = std::env::current_exe().ok();
    inventory_candidates(exe.as_deref())
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
}

/// Lines for the models that need attribution (from the manifest).
#[must_use]
pub fn model_attributions() -> Vec<String> {
    models::Manifest::embedded()
        .map(|m| {
            m.models
                .iter()
                .filter(|e| e.needs_attribution())
                .map(|e| format!("{} ({}): {}", e.title, e.license, e.attribution))
                .collect()
        })
        .unwrap_or_default()
}

/// The text blocks of the window, `(title, body)`.
#[must_use]
pub fn blocks() -> Vec<(String, String)> {
    let mut out = vec![
        (
            t!("about.licence").to_string(),
            t!("about.licence_body", version = env!("CARGO_PKG_VERSION")).to_string(),
        ),
        (
            t!("about.source_offer").to_string(),
            SOURCE_OFFER.to_owned(),
        ),
        (t!("about.notice").to_string(), NOTICE.to_owned()),
    ];
    let attributions = model_attributions();
    if !attributions.is_empty() {
        out.push((t!("about.models").to_string(), attributions.join("\n")));
    }
    out.push((
        t!("about.inventory").to_string(),
        read_inventory().unwrap_or_else(|| t!("about.inventory_missing").to_string()),
    ));
    out
}

/// Opens (or focuses) the About window.
pub fn open(cx: &mut crate::ui::App) {
    crate::windows::open(
        cx,
        "about",
        t!("about.title").to_string(),
        (760.0, 640.0),
        |_, cx| cx.new(|_| AboutView),
    );
}

/// The About view.
#[derive(Debug)]
pub struct AboutView;

impl Render for AboutView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("about")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(palette::BOSQUE))
            .text_color(rgb(palette::MARFIL))
            .p(px(24.0))
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(22.0))
                    .child(t!("about.title").to_string()),
            )
            .child(div().text_color(rgba(0xF4F1E8A0)).child(format!(
                "{} {}",
                t!("app.name"),
                env!("CARGO_PKG_VERSION")
            )))
            .children(blocks().into_iter().map(|(title, body)| {
                v_flex()
                    .gap(px(4.0))
                    .child(div().text_size(px(15.0)).child(title))
                    .child(
                        div()
                            .p(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(palette::BOSQUE_DEEP))
                            .font_family(crate::fonts::FONT_MONO)
                            .text_size(px(11.0))
                            .child(body),
                    )
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_the_notice_and_the_gpl_source_offer() {
        assert!(SOURCE_OFFER.contains("pando-tts") && SOURCE_OFFER.contains("GPL-3"));
        assert!(NOTICE.contains("espeak-ng"));
        let b = blocks();
        assert!(b.len() >= 4);
        assert!(
            b.iter()
                .all(|(t, body)| !t.starts_with("about.") && !body.trim().is_empty())
        );
    }

    #[test]
    fn models_with_attribution_are_listed() {
        let a = model_attributions();
        assert!(a.iter().any(|l| l.contains("Whistle")), "{a:?}");
    }

    #[test]
    fn inventory_is_searched_next_to_the_binary_first() {
        let c = inventory_candidates(Some(Path::new("/opt/pando/pando-assistant")));
        assert_eq!(c[0], PathBuf::from("/opt/pando/legal").join(INVENTORY_FILE));
        assert!(c.last().is_some_and(|p| p.starts_with("/usr/share/doc")));
    }
}
