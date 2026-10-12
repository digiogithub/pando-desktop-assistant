---
created_at: 2026-10-12T09:04:27.540440178Z
updated_at: 2026-10-12T09:04:27.540440178Z
tags:
    - fix
---
# Voice stack outlives the main window (PDA-T-0171)

Follow-up of [[pando-assistant/fixes/post-merge-stabilization.md]].

What: `crates/app/src/voice/host.rs` (`VoiceHost`, `keep_alive`, `is_alive`) holds strong handles to VoiceController, Assistant, ClaudeBridge and OrganismPane; `views/main_view.rs::MainView::new` registers them; `app.rs` builds the stack headlessly when started hidden in background mode (window opened then removed). `voice/controller.rs::is_reachable` added.
Why: tray/hotkey/CLI PTT, hands-free, mute and the Voice/Audio settings sections died with the window in background mode.
Verified: test `the_voice_stack_outlives_the_main_window`; fmt, clippy -D warnings, workspace tests, app tests x5, check-deps, xvfb smoke. Real hotkey+tray session not exercised.
