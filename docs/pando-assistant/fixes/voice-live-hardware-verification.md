---
created_at: 2026-10-12T11:03:54.180165259Z
updated_at: 2026-10-12T11:03:54.180165259Z
tags:
    - fix
    - voice
    - verification
---
# Voice verified on real hardware (PipeWire, USB mic, speakers)

Builds on [[pando-assistant/plans/backlog-execution_plan.md]], [[docs/spikes/S4-audio-cpal-rubato-sonora]] and [[docs/spikes/S5-wake-word]].

## What changed
- `audio::device::low_latency_config`: capture and playback request a fixed 20 ms device buffer (fallback to the backend default). The PulseAudio host otherwise used a 2 s buffer: capture start 2033 ms -> 49 ms, output latency 2000 ms -> 30-40 ms, stop to silent callback 26 ms -> 6 ms. Files: crates/audio/src/{device,playback,capture}.rs.
- `pando-stt` server: Needle's `stream_process` returns only the text committed by that call and `stream_stop` only the tail, so the final transcript lost everything but the last words. `StreamText` in crates/pando-stt/src/server.rs accumulates committed text and words per stream id; partials now show committed + pending. Test: `a_stream_returns_all_committed_text_not_only_the_tail`.
- `audio::bargein::BargeIn::GRACE_MS = 400`: speech during the first 400 ms of a playback is ignored (AEC3 leaks 200-250 ms of residual while it converges on a loopback, which ducked the output). Tests added.
- app voice: `first_audio` stage is marked at `PlaybackStarted` (was at the end of the speech); debug logs for worker messages and barge-in.
- New ignored tests: `crates/audio/tests/hardware.rs` (60 s soak + stall watchdog, loopback RMS alignment), `crates/app/tests/live_loop.rs` (TTS + user over a null-sink loop). Example `crates/pando-tts/examples/say.rs` (text to WAV).

## Verification (this box: LCS USB Audio mic, HDA Intel speakers, PipeWire 1.6.8)
- Capture 60 s: 3001 frames (50/s), 0 restarts; 10 min soak result in the task comment. Stall (SIGSTOP of pipewire-pulse for 3 s): `Restarted { reason: "stalled" }` and recovery. `pactl suspend-source` does not stall an active stream.
- Piper first chunk 128 ms cold / 119 ms warm; Whistle fixture test green; wer-gate green.
- Real app under xvfb, `pando-assistant ptt start/stop` with a null sink as microphone: live caption, full final transcript, decision card, TTS answer, `voice turn` stats (transcript 286-553 ms, first_audio 76-111 ms), worker `kill -9` restarted in ~3 s, PTT barge-in stops playback 0.3 ms after the IPC event.
- Hands-free "Oye Pando" through the virtual mic: WakeWord + Utterance; negatives silent; 4/8 S5 positive clips woke.
- AEC loopback: 0 self-interruptions in 4 s of TTS echo; user speech over TTS stops playback after 811 ms.

## Still needs a human / other hardware
Physical hot-plug, real room acoustics (ERLE, 5 min over speakers, FRR at 2 m), F9/global shortcut on a desktop session, macOS/Windows permission flows. `PDA-T-0095` target (< 150 ms after release) is not met: 286-553 ms.
