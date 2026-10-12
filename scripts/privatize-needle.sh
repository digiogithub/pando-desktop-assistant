#!/usr/bin/env bash
# Linux only. Makes libneedle's statically linked libc++ symbol-private so that the binary can also
# contain libstdc++ code (sherpa-onnx / onnxruntime, i.e. the Parakeet fallback and the VAD in
# `pando-stt`). Without this the static libc++abi interposes __cxa_throw / operator new, which
# aborts C++ exceptions and mismatches new/delete (spike S3, risk R4).
#
# usage: scripts/privatize-needle.sh [vendor/needle/linux-x86_64]
# in:  libneedle.a + lib/libc++.a (+ lib/libc++abi.a)      out: libneedle_private.a
# `needle-sys/build.rs` links libneedle_private.a automatically when it exists.
set -euo pipefail
D="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/vendor/needle/linux-x86_64}"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
cd "$D"
[[ -f libneedle.a && -f lib/libc++.a ]] || { echo "need $D/libneedle.a and $D/lib/libc++.a" >&2; exit 1; }
EXTRA=()
[[ -f lib/libc++abi.a ]] && EXTRA+=(lib/libc++abi.a)
ld -r -o "$T/all.o" --whole-archive libneedle.a --no-whole-archive lib/libc++.a "${EXTRA[@]}"
nm "$T/all.o" | awk '$2=="T" && $3 ~ /^needle_/ {print $3}' > "$T/keep.txt"
[[ -s "$T/keep.txt" ]] || { echo "no needle_* symbols found" >&2; exit 1; }
# The personality routine is referenced through a COMDAT symbol; rename it so it can stay
# global without merging with libstdc++'s.
objcopy --redefine-sym __gxx_personality_v0=__pnd_needle_gxx_personality_v0 \
        --redefine-sym DW.ref.__gxx_personality_v0=DW.ref.__pnd_needle_gxx_personality_v0 \
        "$T/all.o" "$T/ren.o"
echo DW.ref.__pnd_needle_gxx_personality_v0 >> "$T/keep.txt"
objcopy --keep-global-symbols="$T/keep.txt" "$T/ren.o" "$T/priv.o"
rm -f libneedle_private.a
ar rcs libneedle_private.a "$T/priv.o"
echo "wrote $D/libneedle_private.a ($(wc -l < "$T/keep.txt") exported symbols)"
