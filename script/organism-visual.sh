#!/usr/bin/env bash
# PDA-T-0041: SSIM of the tiny-skia organism render against Chrome running organism.html.
# Usage: script/organism-visual.sh [out-dir]   (CHROME=/path/to/chrome to override)
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-target/organism-visual}"
mkdir -p "$out"
node crates/organism/harness/chrome_capture.mjs "$out"
PANDO_CHROME_CAPTURE_DIR="$(realpath "$out")" \
  cargo test -p app --release raster_matches_chrome_capture -- --ignored --nocapture
