---
created_at: 2026-10-12T11:47:13.592034909Z
updated_at: 2026-10-12T11:47:13.592034909Z
tags:
    - change
    - voice
---
# PTT release latency, wake live test, dock settings overlap

Builds on [[pando-assistant/fixes/voice-live-hardware-verification.md]], [[pando-assistant/plans/backlog-execution_plan.md]], [[docs/spikes/S5-wake-word]].

## What changed
- PDA-T-0095: `pando-stt` push-to-talk no longer uses Needle `stream_process`/`stream_stop`. New `LiveStream` (crates/pando-stt/src/live.rs) accumulates audio, cuts at pauses (>=360 ms) or 4 s, decodes each finished phrase once with one-shot `transcribe`, and at release decodes only the open phrase. server.rs `StreamChunk`/`StreamStop` use it. Works for any engine (Parakeet had no streaming).
- Why: Needle's stream cost grows with the audio already fed (25 ms -> 800 ms per 0.75 s chunk after 13 s), so the worker lagged real time; stream_stop itself costs 0 ms.
- Measured (release, null-sink mic, `crates/app/tests/ptt_latency.rs`, release at clip end): 2.7 s clip P50/P95 89/96 -> 35/48 ms; 13.6 s clip 1453/1551 -> 51/64 ms. Target < 150 ms met on the worker path.
- Dock inline voice/audio settings panel capped at 240 px and scrollable (crates/app/src/voice/settings_panel.rs `settings_view`); previously it pushed the dock row out and drew over the view. Verified with xvfb screenshot.
- PDA-T-0138: `crates/app/tests/live_wake.rs`. The 4/8 wake result was an artifact of the 8 s command window after a bare wake phrase. Honouring it: 21/24 positives, 2/9 confusers, same as offline wake-eval. No threshold change.

## Verification
pando-stt tests 49 pass; clippy -D warnings on pando-stt and app; ignored live tests run on this box; pactl null sink unloaded.
Not covered: GPUI event pump delay after the worker's reply (check `voice turn` logs on a desktop session).
