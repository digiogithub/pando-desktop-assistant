#!/usr/bin/env bash
# Build sherpa-onnx from source with TTS OFF, for `pando-stt`'s Parakeet fallback (ADR 0007).
# The prebuilt sherpa-onnx archives embed espeak-ng (GPL-3) even for VAD/ASR-only use, so
# `pando-stt` must not use them. Output: $OUT/lib (set SHERPA_ONNX_LIB_DIR to it).
#
# KNOWN ISSUE (PDA-T-0093): the onnxruntime.a that sherpa-onnx fetches was built with GCC 11 (RHEL
# devtoolset). Building sherpa-onnx itself with a different GCC (tried 12 and 13 on Ubuntu 24.04)
# produces a binary that aborts with `free(): invalid pointer` inside ORT's std::regex
# (DeviceDiscovery) because both sides emit weak std::regex instantiations with different layouts.
# Build in a GCC 11 environment (manylinux2014 / devtoolset-11 container, as upstream CI does) or
# build onnxruntime from source with the same compiler. Verified: the Rust code path itself works
# against the upstream prebuilt archives (development only, never distributed: they embed espeak-ng).
#
# usage: scripts/build-sherpa-stt.sh [OUT_DIR]      (default ~/.cache/pando-assistant/sherpa-stt)
# then:  SHERPA_ONNX_LIB_DIR=<OUT_DIR>/lib cargo build -p pando-stt --features parakeet
set -euo pipefail
VERSION=1.13.8
OUT="${1:-$HOME/.cache/pando-assistant/sherpa-stt}"
SRC="$OUT/src"
mkdir -p "$OUT"
[[ -d $SRC ]] || git clone --depth 1 --branch "v$VERSION" https://github.com/k2-fsa/sherpa-onnx.git "$SRC"
cmake -S "$SRC" -B "$OUT/build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$OUT/install" \
  -DBUILD_SHARED_LIBS=OFF -DSHERPA_ONNX_ENABLE_TTS=OFF -DSHERPA_ONNX_ENABLE_SPEAKER_DIARIZATION=OFF \
  -DSHERPA_ONNX_ENABLE_BINARY=OFF -DSHERPA_ONNX_ENABLE_PORTAUDIO=OFF -DSHERPA_ONNX_ENABLE_WEBSOCKET=OFF \
  -DSHERPA_ONNX_ENABLE_C_API=ON -DSHERPA_ONNX_BUILD_C_API_EXAMPLES=OFF -DSHERPA_ONNX_ENABLE_TESTS=OFF \
  -DSHERPA_ONNX_ENABLE_PYTHON=OFF
cmake --build "$OUT/build" -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu)"
cmake --install "$OUT/build"
mkdir -p "$OUT/lib"
cp "$OUT"/install/lib/*.a "$OUT/lib/"
# onnxruntime static lib is fetched by the build; copy it next to the others.
find "$OUT/build" "$OUT/install" -name 'libonnxruntime*.a' -exec cp -n {} "$OUT/lib/" \;
# sherpa-onnx-sys links the TTS-era libs by name; a TTS-off build has no such objects,
# so provide empty archives (they must NOT contain espeak-ng).
for l in piper_phonemize espeak-ng ucd; do
  [[ -f $OUT/lib/lib$l.a ]] || ar rcs "$OUT/lib/lib$l.a"
done
echo "SHERPA_ONNX_LIB_DIR=$OUT/lib"
ls -la "$OUT/lib"
