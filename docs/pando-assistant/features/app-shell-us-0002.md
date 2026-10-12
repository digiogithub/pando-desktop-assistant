---
created_at: 2026-10-11T22:42:52.784236033Z
updated_at: 2026-10-11T22:42:52.784236033Z
tags:
    - feature
    - app
    - shell
---
# App shell (PDA-US-0002: T-0005..T-0009)

Part of [[pando-assistant/plans/backlog-execution_plan.md]]. Builds on [[adr/0001-gpui-kit-pin-and-ui-facade]], [[adr/0003-rust-i18n]] and the Bitacora reference in [[docs/research/bitacora-gpui.md]].

## What changed
- `crates/app` is now lib `app` + bin `pando-assistant`.
- T-0005 `ui/mod.rs` facade (explicit re-exports, application/init/open_main_window/notify/confirm/key_binding/canvas/frameless/testing); test `ui::tests::only_ui_names_gpui_kit`. `ui/kit_theme.rs` installs the generated `assets/themes/pando.json` (all 71 keys verified against gpui-component 0.7.1) and a gpui_test checks palette colours.
- T-0006 `tokio_bridge.rs` (global runtime, AbortOnDrop spawn, 2 s shutdown) and `events.rs` (`EventPump::attach`, burst drain up to 256 events, one `cx.notify()` per batch).
- T-0007 `fonts.rs` + `assets/fonts/` (Space Grotesk 400/500/700, JetBrains Mono 300/400, Zen Kaku Gothic New 300, OFL texts, `NOTICE`); `ui/assets.rs` `AppAssets` serves brand SVGs for `img()`; wordmarks get an injected intrinsic size because GPUI rasterizes SVGs at 2x and the 4132-unit viewBox exceeds the 8192 px cap.
- T-0008 `paths.rs`, `logging.rs`, `settings.rs` (tolerant, atomic, corrupt backup), `i18n.rs` + `assets/locales/app.{es,en}.yml`, `cli.rs`, `app.rs` (`--smoke-test`).
- T-0009 `views/title_bar.rs` (52 px bar, client decorations, granted-decoration branching), `views/main_view.rs`.

## Verification
fmt, `clippy --workspace --all-targets -D warnings`, `cargo test -p app` (33 tests) pass. `WAYLAND_DISPLAY= xvfb-run pando-assistant --smoke-test` exits 0; Xvfb screenshot shows title bar, two-tone wordmark and chips.

## Gotchas
- Linking needs `libxkbcommon-x11.so` (only `.so.0` installed); use `LIBRARY_PATH` to a dir with the symlink.
- On the busy interactive desktop the smoke window is inactive/obscured and GPUI delivers no frames (watchdog exit 2); use Xvfb with `WAYLAND_DISPLAY=` unset.
- Cross-platform move/resize (GNOME Wayland, KDE, macOS, Windows) not verified: T-0009 left in_review.
