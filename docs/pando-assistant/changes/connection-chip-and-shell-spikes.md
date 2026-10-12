---
created_at: 2026-10-11T23:04:15.192718941Z
updated_at: 2026-10-11T23:04:15.192718941Z
tags:
    - change
    - app
    - spike
---
# Connection chip (T-0048) and shell spikes (T-0024/25/26)

Part of [[pando-assistant/plans/backlog-execution_plan.md]]. Builds on [[pando-assistant/features/app-shell-us-0002]] and [[pando-assistant/changes/pando-client-core]].

## What changed
- T-0048: `crates/app/src/connection.rs` (`ConnectionHub`, `chip_spec`, `needs_empty_state`), `views/main_view.rs` (chip + "sin Pando" screen), locales `pando.*`, `AppSettings.pando_url`, `ui::input` facade, app depends on `pando-client`. Verified by tests, clippy, Xvfb screenshot; not against a live Pando.
- Spikes (throwaway, `spikes/gpui-shell`, `CARGO_TARGET_DIR=target/spikes`): reports `docs/spikes/T-0024-gpui-paths-performance.md`, `T-0025-always-on-top-mini-window.md`, `T-0026-global-hotkey-and-tray.md`.

## Key findings
- Direct GPUI paths cost ~1 ms CPU/frame for the organism-sized scene; tiny-skia only as software-GPU fallback.
- GPUI has no layer-shell margin setter; `PopUp` on Wayland is a plain toplevel; GNOME lacks layer-shell.
- ashpd's default `tokio` feature breaks the build next to GPUI's `async-io`: use `default-features = false, features = ["async-io"]` for ashpd and ksni.
- On the live desktop session, background windows get no frames (use `WAYLAND_DISPLAY= xvfb-run`).
