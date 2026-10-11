# Spike S4 · Audio with cpal 0.18 + rubato 5 + sonora AEC3

- **Task:** PDA-T-0021 (story PDA-US-0006) · **Date:** 2026-10-12 · **Status:** Linux/PipeWire done; acoustic ERLE, 5-minute run and other OSes pending
- **Code:** [`spikes/s4-audio-aec/`](../../spikes/s4-audio-aec/) (standalone crate, excluded from the workspace); barge-in VAD uses `s3-sherpa-tts vadfile` from [S3](S3-sherpa-onnx-single-runtime.md)
- **Builds on:** [`docs/research/voice-stack.md`](../research/voice-stack.md) §4; [`docs/02-voice-pipeline.md`](../02-voice-pipeline.md); [ADR 0004](../adr/0004-voice-worker-separate-process.md)

## TL;DR

| Question | Answer |
|---|---|
| cpal 0.18.2 capture + playback on PipeWire | **Works.** Default host is PipeWire when the `pipewire` feature is on; 48 kHz stereo f32, 1024-frame (21.3 ms) callbacks both ways, 0 errors/xruns in 2 × 12 s duplex runs. |
| rubato 5.0.1 `Fft` 48 → 16 kHz | Passband flat (0.0 dB at 1 and 6 kHz), aliases rejected **≥ 105 dB**; **1.4 µs per 10 ms** chunk; delay 5 ms + 10 ms chunking. |
| sonora 0.2.0 AEC3 at 16 kHz mono | **40 µs per 10 ms frame (0.4 % of one core)**. With a synthetic room echo on top of the real mic noise floor: **ERLE 23–31 dB**, **0 false barge-ins** after AEC (raw mic: echo always detected as speech). |
| Barge-in during TTS (double talk) | Detected **~320 ms after onset** when the echo is ≥ 20 dB below the speaker signal; **not detected** while TTS plays when echo is only 10 dB down (AEC3 suppresses the near-end by ~10 dB). PTT barge-in stays the always-available path. |
| Device changes | Stream on the *default* device follows a default switch (`DeviceChanged`, no rebuild). A stream on an explicit device that disappears **reports no error**: PipeWire host stops calling back (silent stall), Pulse host gets re-routed by the server. **A callback watchdog + rebuild (8 ms) recovers.** |
| Real acoustic ERLE on this box | **Not measurable here**: the USB mic does not pick up the desktop speakers (mic stays at its −58 dBFS noise floor with TTS at −17 dBFS). Pending on a laptop. |

**Decisions for implementation** (crates `app` audio module and `voice-worker`):
1. cpal with features `["pipewire", "pulseaudio"]`; PipeWire default, Pulse/ALSA selectable.
   Open the device's native config (48 kHz f32) and resample; never request 16 kHz.
2. rubato `Fft::<f32>::new(48_000, 16_000, 480, 1, FixedSync::Input)` per capture stream
   (10 ms chunks, delay 5 ms). Use `Async::new_sinc` only for odd device rates / drift.
3. sonora AEC3 at 16 kHz mono, 10 ms frames, render = **the exact PCM handed to the output
   callback** (resampled to 16 kHz), capture = mic. Hands-free barge-in = Silero VAD on the AEC
   output, ≥ 200 ms speech, then duck → stop (as `voice-stack.md` §4.5). Enable hands-free barge-in
   only after a short calibration shows echo ≥ 20 dB below the reference (or a headset); otherwise
   PTT-only.
4. Audio supervisor: rebuild the stream on `DeviceNotAvailable`/`StreamInvalidated` **and** when no
   callback arrived for 500 ms (PipeWire host stall). Re-create the `Host` to see new devices
   (the PipeWire host snapshots its device list at creation).

## Environment

Same box as S3 (Intel Core Ultra 9 285, Pop!_OS 24.04, PipeWire 1.6.8 with pipewire-pulse).
Output: HDA Intel PCH analog, **speaker** port, volume 57 %. Input: "LCS USB Audio" mono USB mic.
`cpal = "=0.18.2"` (features `pipewire`, `pulseaudio`), `rubato = "=5.0.1"`, `sonora = "=0.2.0"`,
`rtrb 0.4`, `hound 3`, `audioadapter-buffers 5.1`.

### Build requirements found

- cpal's **PipeWire and PulseAudio hosts are opt-in features**; default features give ALSA only.
  The research note's "default host priority PipeWire → Pulse → ALSA" holds only with the features on.
- `pipewire` feature needs `libpipewire-0.3-dev`, `libspa-0.2-dev` **and libclang** (bindgen in
  `libspa-sys`); ALSA always needs `libasound2-dev`. The `pulseaudio` feature is a pure-Rust client
  (no system lib). This box had none of the dev packages and no sudo, so the spike used extracted
  `.deb`s (`apt-get download` + `dpkg-deb -x` into `~/.cache/pando-assistant-spikes/sysdev`):

  ```bash
  export PKG_CONFIG_PATH=$SYSDEV/usr/lib/x86_64-linux-gnu/pkgconfig   # .pc prefix rewritten
  export LIBRARY_PATH=$SYSDEV/usr/lib/x86_64-linux-gnu                # libasound.so, libpipewire-0.3.so symlinks
  export LIBCLANG_PATH=$SYSDEV/usr/lib/llvm-18/lib                    # from libclang1-18
  export BINDGEN_EXTRA_CLANG_ARGS="-I/usr/lib/gcc/x86_64-linux-gnu/14/include"   # stdbool.h
  ```

  CI and packaging must install `libasound2-dev libpipewire-0.3-dev libspa-0.2-dev libclang-dev`
  (Debian/Ubuntu) or the Fedora equivalents. The runtime needs only `libasound.so.2` and
  `libpipewire-0.3.so.0`, present on any PipeWire desktop.

## Measurements

### Hosts and devices (`s4-audio-aec devices`)

| Host | Init | Default input | Default output |
|---|---|---|---|
| **PipeWire** (default) | 1 ms | `default_input` 48 kHz, 2 ch, **F32**, buffer 32–2048 | `default_output` 48 kHz, 2 ch, F32, buffer 32–2048 |
| PulseAudio | 3 ms | LCS USB Audio, 48 kHz, 1 ch, **I24** | HDA Intel, 48 kHz, 2 ch, **I32** |
| ALSA | 0 ms (113 devices) | `default` 48 kHz F32 | `default` 48 kHz F32 |

The Pulse host reports integer default formats; the audio module must convert (cpal's
`SizedSample` path) rather than assume f32.

### Duplex capture + playback (`s4-audio-aec duplex pipewire …`)

Plays the S3 Piper paragraph (10.7 s, resampled 22.05 → 48 kHz with rubato in 2 ms) while
recording the mic; the output callback pushes exactly what it plays into an `rtrb` ring (the AEC
reference), the input callback pushes mic samples into another. No allocation or locking in the
callbacks.

| Metric | Value |
|---|---|
| Callback size, out / in | 1024 / 1024 frames (21.3 ms), constant, 558 callbacks each in 12 s |
| Reported output latency (`playback − callback`) | 42.7 ms |
| Reported input latency (`callback − capture`) | 0.0 ms (PipeWire host does not report capture latency) |
| Errors / xruns | none |

### Resampling, rubato 5 `Fft` 48 → 16 kHz (`s4-audio-aec resample`)

| Input tone | Output level (relative) |
|---|---|
| 1 kHz | 0.0 dB |
| 6 kHz | −0.0 dB |
| 7.6 kHz (transition band) | −61.1 dB |
| 9 kHz (alias → 7 kHz) | −126.8 dB |
| 12 kHz (alias → 4 kHz) | −130.2 dB |
| 20 kHz (alias → 4 kHz) | −105.2 dB |

Passband is flat to at least 6 kHz and cut before 7.6 kHz: right for 16 kHz STT/VAD. Streaming
cost: **1.4 µs per 480-frame (10 ms) call**, `output_delay` 80 frames (5 ms) plus 10 ms chunk
buffering. With 1024-frame chunks: 7.3 µs per call, 2.7 ms delay but 21.3 ms buffering.

### Echo cancellation, sonora AEC3 (`s4-audio-aec aec …`)

Offline on the duplex recording, both signals resampled to 16 kHz, reference pre-aligned with
the stream timestamps (`playback0 − capture0` = 42.7 ms), 10 ms frames, default `EchoCanceller`.

**Real room:** the USB mic does not hear the speakers. The mic stays at its noise floor
(−58 dBFS) whether the TTS plays at −27 or −17 dBFS; cross-correlation finds no stable peak.
Measured "ERLE" there (2.7–3.5 dB) is just noise reduction, not echo cancellation. **Real
acoustic ERLE is pending** on a machine whose mic hears its speakers (a laptop).

**Synthetic echo path** (`S4_SYNTH_ERL_DB`): the real mic recording (noise floor) plus the played
reference through a deterministic room impulse response (60 ms bulk delay, RT60 ≈ 300 ms) and a
`tanh` loudspeaker nonlinearity, at a given echo return loss (ERL). Barge-in = Silero VAD v5
(`threshold 0.5`, `min_speech 0.2 s`) over the AEC output.

| ERL (echo below reference) | Mic level | AEC out | Measured ERLE (100 ms far-end-only blocks) | sonora-reported ERLE | VAD segments, raw mic | VAD segments, after AEC |
|---|---|---|---|---|---|---|
| 20 dB | −37.7 dBFS | −60.8 dBFS | **23.5 dB** | 9.7 dB | 1 (whole TTS) | **0** |
| 10 dB | −27.7 dBFS | −58.7 dBFS | **31.1 dB** | 10.8 dB | 1 | **0** |
| 6 dB | −23.7 dBFS | −52.6 dBFS | **28.6 dB** | 10.5 dB | 1 | **0** (also 0 at VAD threshold 0.3) |

Per-block ERLE converges within the first second (first blocks 22–36 dB) with dips to 4–12 dB at
transients. sonora's own `echo_return_loss_enhancement` counts only the linear filter; ours
includes the residual echo suppressor.

**Double talk** (near-end = Kokoro `em_alex` speech mixed in at 4.0 s, onset 4.20 s):

| ERL | Near-end gain | Near-end kept by AEC | First barge-in detection (VAD thr 0.5) |
|---|---|---|---|
| 30 dB | 0.3 / 0.1 | −3.3 / −5.2 dB | **4.51 s (+310 ms)** |
| 20 dB | 0.3 / 0.1 | −5.1 / −7.8 dB | **4.54 s (+340 ms)** |
| 10 dB | 0.3 / 0.1 / 0.05 | −9.7 / −9.8 / −7.9 dB | **only at 11.6 s, after TTS ended** |

`+310–340 ms` = 200 ms `min_speech_duration` + VAD windowing + AEC adaptation. With strong
(10 dB) echo the residual echo suppressor removes the near-end too, so hands-free barge-in must
be gated by a calibration (see decisions) and PTT barge-in is the fallback.

CPU: **40 µs per 10 ms frame** at 16 kHz mono (0.4 % of one core).

### Device changes (`s4-audio-aec hotplug pipewire|pulseaudio`)

A null sink is created with `pactl load-module module-null-sink`, made default, switched back,
then a stream is opened on it explicitly and the module is unloaded (simulated unplug).

| Scenario | PipeWire host | PulseAudio host |
|---|---|---|
| Stream on default device; default changes twice | `DeviceChanged` × 2, stream keeps running | no event (server moves the stream) |
| New device visible to an existing `Host` | **No**: the device list is a snapshot; a new `Host` sees it | Yes |
| Stream on explicit device, device removed | **No error, callbacks stop** (0 samples in 500 ms) | No error; server re-routes, callbacks continue |
| Recovery | Watchdog (no callback for 500 ms) → rebuild on default: **8 ms**, audio flows again | not needed |

So the documented `DeviceNotAvailable` error did not fire for a removed virtual sink on either
host. Real Bluetooth/USB unplug may behave differently: **pending a physical test**.

## Acceptance criteria status

| Criterion | Status |
|---|---|
| ERLE measured | **Partial:** 23–31 dB on a synthetic echo path; real acoustic ERLE pending (mic here does not hear the speakers). |
| 0 false barge-ins in 5 min of TTS over speakers | **Partial:** 0 false barge-ins in every far-end-only run (4 × 11 s synthetic, 2 × 11 s real); a 5-minute loud run was not done (short-test rule; needs a laptop with an echo path). |
| Device hot-plug recovered | **Simulated:** default switch survives; removed device recovered by watchdog + rebuild (8 ms). Physical USB/BT unplug pending. |
| Pulse, CoreAudio, WASAPI | Pulse host enumerates and re-routes (duplex not run: integer formats). CoreAudio/WASAPI pending. |
