---
created_at: 2026-10-12T08:08:31.193056868Z
updated_at: 2026-10-12T08:08:31.193056868Z
tags:
    - feature
    - shell
    - PDA-EP-0008
---
# Desktop shell: single instance, PTT triggers, tray, mini window, notifications (PDA-EP-0008 part)

Part of [[pando-assistant/plans/backlog-execution_plan.md]]. Builds on spikes T-0025 / T-0026 (docs/spikes) and [[pando-assistant/features/cells-us-0014-0016]].

## What changed
New `crates/app/src/shell/`: `ipc` (single instance + loopback IPC, `pando-assistant ptt|show|hide|toggle-mini|quit`), `controller` (inbox `Msg`, pure `ShellState` -> `Effect`s), `ptt` (idempotent hold-to-talk machine), `hotkey` (global-hotkey press/release), `portal` (GlobalShortcuts via ashpd/async-io, session detection, compositor fallback lines), `tray` (ksni / tray-icon, pure menu + icon), `autostart`, `mini` (floating window per platform), `keymap` (ToggleMini / ShowMain actions + keymap.json), `notify` (toasts + OS notifications policy), `runtime` (GPUI side). Settings gained `shell: ShellSettings`. App wiring in `app.rs` (single instance, opener, `--minimized`), `title_bar.rs` close -> `runtime::close_window`, `cells/pane.rs` publishes cells/pending to the shell and starts the toast stream.

## Why
PDA-T-0118..0125, 0128: hold-to-talk from any app, tray/background mode, mini window, notifications.

## Verification
`cargo test -p app` (119), clippy `-D warnings`; xvfb: second launch focuses and exits, `ptt start/stop` and X11 hotkey (xdotool) produce Started/Stopped, Ctrl+Shift+M opens the mini window (screenshot), quit and "not running" exit codes. Not verified: Windows/macOS (tray-icon, global-hotkey, notifications, registry; gpui cannot be cross-checked for Windows here), KDE/GNOME portal, StatusNotifier tray, Sway release binding.

## User notes
Sway: `bindsym --no-repeat Ctrl+Mod1+space exec pando-assistant ptt start` and `bindsym --release Ctrl+Mod1+space exec pando-assistant ptt stop`. User keymap: `<config>/keymap.json`.
