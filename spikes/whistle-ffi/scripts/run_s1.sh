#!/usr/bin/env bash
# Runs every S1 configuration; results in $S/results/*.jsonl, summary on stdout.
set -euo pipefail
S=${SPIKE:-$HOME/.cache/pando-assistant-spikes}
HERE=$(cd "$(dirname "$0")" && pwd)
B=${BENCH:-$HERE/../../../target/spikes/release/bench}
PY="$S/venv/bin/python -I"
R=$S/results; mkdir -p "$R"
KW=$HERE/keywords_es.txt
run() { local name=$1; shift; [ -s "$R/$name.jsonl" ] || "$@" > "$R/$name.jsonl"; }
for set in fleurs mls cmd cmdnoisy lat3 lat10; do
  L=$S/sets/list_$set.tsv
  run whistle_es_$set "$B" "$S/needle/whistle.cact" "$L" --lang es
  run parakeet_$set $PY "$HERE/sherpa_bench.py" "$S" parakeet "$L" 4
  run whisper-small_$set $PY "$HERE/sherpa_bench.py" "$S" whisper-small "$L" 4
done
for set in cmd cmdnoisy; do
  L=$S/sets/list_$set.tsv
  run whistle_auto_$set "$B" "$S/needle/whistle.cact" "$L"
  run whistle_es_kw_$set "$B" "$S/needle/whistle.cact" "$L" --lang es --keywords "$KW"
done
for f in "$R"/*.jsonl; do
  n=$(basename "$f" .jsonl); set=${n##*_}
  extra=(); case $set in cmd|cmdnoisy) extra=(--keywords "$KW");; esac
  echo "$n $($PY "$HERE/wer.py" "$S/sets/refs_$set.tsv" "$f" "${extra[@]}")"
done
