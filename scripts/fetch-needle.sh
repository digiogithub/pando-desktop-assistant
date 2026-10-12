#!/usr/bin/env bash
# Fetch the binary-only Needle/Whistle engine into vendor/needle/<platform>/ (never committed).
#
# usage: scripts/fetch-needle.sh [platform ...] [--model]
#   platform: linux-x86_64 | linux-arm64 | macos-arm64 | windows-x86_64   (default: host)
#   --model : also fetch whistle.cact into vendor/needle/models/ (16.9 MB)
# Env: NEEDLE_VENDOR_DIR overrides the destination root (default <repo>/vendor/needle).
# Pins: Hugging Face commit SHAs and SHA-256 (docs/research/voice-stack.md section 6.2).
set -euo pipefail

NEEDLE_COMMIT=93e88ede867e4311c39151285e74df513e4b835b
WHISTLE_COMMIT=ca5287601bef25af26dcf2e1b2bdc0843a7c19e5
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${NEEDLE_VENDOR_DIR:-$ROOT/vendor/needle}"

lib_sha() {
  case "$1" in
    linux-x86_64)   echo 5f8ed375f304cf7e7ba9e6e37d105b51549597e014afbd006407a0836804e5cd ;;
    linux-arm64)    echo 3847620903354d3965c52eb319d3b36acf0a7a1cbe09c16d56e9f6b612ab6fbc ;;
    macos-arm64)    echo c1b955778dcb79c92efaf5606f94e9d3a4ed85e093096b41e502bc9748a90614 ;;
    windows-x86_64) echo ef057b317fbc3103758e295052a5c717a695589b379e2202aa4a7e879e425672 ;;
    *) echo "unsupported platform '$1' (no macOS x86_64 build exists; use the Parakeet fallback)" >&2; exit 2 ;;
  esac
}
MODEL_SHA=b6e02f048568ac5d01a2042556c658061e699acbc0aa2a1439f52f3d461dffeb
HEADER_SHA256=a27dac4b3239eead91d124b7f7dbce5495c13019903fd817b68c16d39b055a99

host_platform() {
  local os arch
  os=$(uname -s); arch=$(uname -m)
  case "$os-$arch" in
    Linux-x86_64) echo linux-x86_64 ;;
    Linux-aarch64|Linux-arm64) echo linux-arm64 ;;
    Darwin-arm64) echo macos-arm64 ;;
    MINGW*|MSYS*|CYGWIN*) echo windows-x86_64 ;;
    *) echo "no Needle engine for $os-$arch" >&2; exit 2 ;;
  esac
}

sha256_of() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

fetch() { # url dest expected_sha
  local url=$1 out=$2 want=$3
  if [[ -f $out && $(sha256_of "$out") == "$want" ]]; then echo "ok   $out (cached)"; return; fi
  mkdir -p "$(dirname "$out")"
  echo "get  $url"
  curl -fsSL --retry 3 -o "$out.part" "$url"
  local got; got=$(sha256_of "$out.part")
  if [[ $got != "$want" ]]; then
    rm -f "$out.part"
    echo "SHA-256 mismatch for $out: expected $want, got $got" >&2
    exit 1
  fi
  mv "$out.part" "$out"
  echo "ok   $out"
}

platforms=(); model=0
for a in "$@"; do
  case "$a" in --model) model=1 ;; -h|--help) sed -n '2,8p' "$0"; exit 0 ;; *) platforms+=("$a") ;; esac
done
[[ ${#platforms[@]} -eq 0 ]] && platforms=("$(host_platform)")

for p in "${platforms[@]}"; do
  base="https://huggingface.co/Cactus-Compute/needle3/resolve/$NEEDLE_COMMIT/$p"
  fetch "$base/libneedle.a" "$DEST/$p/libneedle.a" "$(lib_sha "$p")"
  fetch "$base/needle.h" "$DEST/$p/needle.h" "$HEADER_SHA256"
done
if [[ $model == 1 ]]; then
  fetch "https://huggingface.co/Cactus-Compute/whistle/resolve/$WHISTLE_COMMIT/whistle.cact" \
    "$DEST/models/whistle.cact" "$MODEL_SHA"
fi
