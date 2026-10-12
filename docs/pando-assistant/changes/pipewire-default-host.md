---
created_at: 2026-10-12T13:04:16.143929604Z
updated_at: 2026-10-12T13:04:16.143929604Z
---
# PipeWire as the default audio host on Linux

Builds on [[pando-assistant/fixes/voice-live-hardware-verification.md]], [[pando-assistant/changes/ptt-latency-wake-test-settings-overlap.md]], [[pando-assistant/plans/backlog-execution_plan.md]].

## What changed
- `audio` default features = pulseaudio + pipewire; `app` default features include `pipewire` (cpal's pipewire dep is Linux-gated, so a no-op elsewhere; `--no-default-features` skips libpipewire/libclang).
- cpal `default_host()` already picks PipeWire only if its daemon is available, else PulseAudio, else ALSA. `audio::device::open_host` additionally honours env `PANDO_AUDIO_HOST` (explicit host setting wins); const `HOST_ENV`. Test `the_host_can_be_forced_by_name_and_env`.
- Flatpak: `--filesystem=xdg-run/pipewire-0`. install-linux-deps.sh comment. Packager deb already depends on libpipewire-0.3-0 (alternative libpulse0). `cargo xtask bundle --formats deb` builds.

## Measurements (PipeWire 1.6.8, LCS USB mic, HDA speakers) PipeWire vs Pulse
- capture start 8 ms vs 41 ms; output latency 21.3 vs 40.6 ms; stop->silence 6 vs 6 ms; 60 s soak 3001 frames, 0 restarts.
- PTT release->final (release build, null sink) P50/P95 31/50 vs 35/39 ms.
- live_loop: 0 barge-ins in 4 s; user speech stop 744 vs 758 ms. live_wake identical (confusers 2/5).
- Device ids become `pipewire:<node>` (loop tests: PANDO_LOOP_IN=pipewire:vmic).

## Verification
fmt, workspace clippy -D warnings (default and --no-default-features), workspace tests, check-deps, cargo deny licenses all green. Fallback verified with PIPEWIRE_REMOTE=/nonexistent and PANDO_AUDIO_HOST=PulseAudio/Alsa. Null sink unloaded, default source unchanged.
