# audio

Capture and playback for the assistant (PDA-T-0084, PDA-T-0109; spike S4). Pure Rust on top of `cpal 0.18`,
`rtrb` rings and `rubato 5`. **No GPUI, no network.** The `app` wires it to the organism and to the voice workers.

- `capture`: native-rate input, allocation- and lock-free callback into an `rtrb` ring, a supervisor thread
  that downmixes, resamples to 16 kHz mono (`rubato` FFT, sinc for odd rates), keeps a 2 s pre-roll, tracks the
  normalised mic level and rebuilds the stream on device change / error / 500 ms callback silence.
- `playback`: queue PCM at the voice rate, resampled to the device rate and fed through an `rtrb` ring;
  `stop()` flips an atomic that makes the next callback drain the ring (well under one buffer); the output RMS
  is measured on the samples the callback actually writes.
- `level`, `preroll`, `resample`: pure helpers with unit tests.

Linux build needs `libasound2-dev` (ALSA is always compiled in); the optional `pipewire` feature needs
`libpipewire-0.3-dev libspa-0.2-dev libclang-dev`. Hardware smoke tests are `#[ignore]`.
