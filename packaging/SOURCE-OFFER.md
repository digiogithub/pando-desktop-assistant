# Corresponding Source of `pando-tts` (GPL-3.0-or-later)

`pando-tts` is distributed as a GPL-3 combined work because it links espeak-ng through
sherpa-onnx (ADR 0007). In accordance with GPL-3 section 6(d), every release that contains
`pando-tts` offers the Corresponding Source at the same place and tag as the binary, as the
asset `pando-assistant-<version>-corresponding-source.tar.gz`, produced by
`cargo xtask source-bundle`. It contains:

- the repository at the release commit (crates `pando-tts`, `speech`, `voice-proto`, the build
  scripts under `scripts/`, `xtask/`, `packaging/`) and `Cargo.lock`;
- `SOURCES.txt`: the exact upstream references used to build the binary: sherpa-onnx tag
  `v1.13.8` (https://github.com/k2-fsa/sherpa-onnx), whose build downloads espeak-ng
  (k2-fsa fork of https://github.com/espeak-ng/espeak-ng) and piper-phonemize
  (https://github.com/csukuangfj/piper-phonemize) at the commits pinned in its
  `cmake/espeak-ng-for-piper.cmake` and `cmake/piper-phonemize.cmake`, and ONNX Runtime at the
  version pinned by `cmake/onnxruntime.cmake`;
- the licence texts (`packaging/licenses/`).

The build instructions are `cargo build --release -p pando-tts` (see `crates/pando-tts/README.md`).
You can replace the installed `pando-tts` with your own build: set its path in the app
settings (the app never refuses a replaced worker).

The other programs (`pando-assistant`, `pando-stt`) are MIT-licensed and contain no GPL code.
The libneedle static library inside `pando-stt` is a prebuilt Apache-2.0 binary and is NOT part of
the `pando-tts` work.
