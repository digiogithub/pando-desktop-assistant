//! Secondary windows (settings, first-run assistant): one instance per id, brought to the front
//! when opened again, and excluded from the shell's "main window" logic.

use std::collections::HashMap;

use crate::ui::{
    AnyWindowHandle, App, Bounds, Entity, Global, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, px, size,
};

#[derive(Default)]
struct ToolWindows(HashMap<&'static str, AnyWindowHandle>);

impl Global for ToolWindows {}

/// Whether `w` is a tool window (not the main one).
#[must_use]
pub fn is_tool_window(cx: &App, w: &AnyWindowHandle) -> bool {
    cx.try_global::<ToolWindows>()
        .is_some_and(|t| t.0.values().any(|h| h.window_id() == w.window_id()))
}

/// Opens (or focuses) the tool window `id`.
pub fn open<V: Render>(
    cx: &mut App,
    id: &'static str,
    title: String,
    dims: (f32, f32),
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) {
    if !cx.has_global::<ToolWindows>() {
        cx.set_global(ToolWindows::default());
    }
    let existing = cx.global::<ToolWindows>().0.get(id).copied();
    if let Some(h) = existing
        && cx.windows().iter().any(|w| w.window_id() == h.window_id())
    {
        let _ = h.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let bounds = Bounds::centered(None, size(px(dims.0), px(dims.1)), cx);
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..TitlebarOptions::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(480.0), px(360.0))),
        app_id: Some(crate::views::title_bar::APP_ID.to_owned()),
        ..WindowOptions::default()
    };
    match crate::ui::open_main_window(options, cx, build) {
        Ok((handle, _)) => {
            cx.global_mut::<ToolWindows>().0.insert(id, handle);
        }
        Err(e) => tracing::warn!("cannot open the {id} window: {e:#}"),
    }
}
