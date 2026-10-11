//! Root view of the main window: frameless chrome (window border + 52 px title bar) over a
//! placeholder body until the organism lands.

use rust_i18n::t;

use crate::ui::assets::WORDMARK_ON_DARK;
use crate::ui::frameless::window_border;
use crate::ui::theme::palette;
use crate::ui::{
    Context, Hsla, IntoElement, ParentElement as _, Render, Styled as _, Window, div, h_flex, px,
    rgb, rgba, text_edit::img, v_flex,
};
use crate::views::title_bar::AppTitleBar;

/// The window contents.
#[derive(Debug, Default)]
pub struct MainView;

fn marfil_alpha(c: palette::Rgba) -> Hsla {
    rgba(c.as_u32()).into()
}

/// A rounded chip with a label (placeholder for the project pill and status chips).
fn chip(label: String) -> impl IntoElement {
    div()
        .px(px(10.0))
        .py(px(3.0))
        .rounded_full()
        .border_1()
        .border_color(marfil_alpha(palette::MARFIL_ALPHA_22))
        .text_color(marfil_alpha(palette::MARFIL_ALPHA_62))
        .text_size(px(11.0))
        .child(label)
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let title_bar = AppTitleBar::new()
            .left(
                img(WORDMARK_ON_DARK)
                    .h(px(22.0))
                    .w(px(84.0))
                    .flex_shrink_0(),
            )
            .left(chip(t!("titlebar.project_none").to_string()))
            .right(
                h_flex()
                    .gap_2()
                    .pr_3()
                    .child(chip(t!("titlebar.status_pando").to_string()))
                    .child(chip(t!("titlebar.status_voice").to_string())),
            );
        window_border().child(
            v_flex()
                .size_full()
                .bg(rgb(palette::BOSQUE))
                .text_color(rgb(palette::MARFIL))
                .child(title_bar)
                .child(
                    div()
                        .flex_1()
                        .p(px(32.0))
                        .child(t!("main.placeholder").to_string()),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};

    #[gpui_test]
    fn main_view_renders_headless_with_the_wordmark(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            crate::ui::kit_theme::install(cx);
        });
        let window = cx.add_window(|_, _| MainView);
        let mut cx = crate::ui::testing::VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}
