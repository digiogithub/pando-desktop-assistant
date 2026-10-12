#!/usr/bin/env bash
# Local STT quality gate (PDA-T-0096). Not run in CI.
# usage: scripts/wer-gate.sh [--update]      needs vendor/needle (scripts/fetch-needle.sh --model)
set -euo pipefail
cd "$(dirname "$0")/.."
export PANDO_STT_MODEL="${PANDO_STT_MODEL:-$PWD/vendor/needle/models/whistle.cact}"
[[ "${1:-}" == "--update" ]] && export WER_UPDATE=1
cargo build -p pando-stt
cargo test -p app --test wer_gate -- --ignored --nocapture
