#!/usr/bin/env bash
# Records raw Pando HTTP/SSE bytes into crates/pando-client/tests/fixtures/.
#
# Starts `pando serve` on a scratch port in a throwaway project dir (or reuses PANDO_URL),
# fetches the loopback token, and replays a handful of tiny prompts (each costs real LLM
# tokens: keep the list short). Tokens are never written to disk: they are scrubbed from
# every captured body. Re-runnable; overwrites the fixtures.
#
# Usage: record-fixtures.sh            (starts its own server on port 18765)
#        PANDO_URL=https://localhost:8765 record-fixtures.sh   (uses a running server)
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="$here/../tests/fixtures"
ca="${PANDO_CA:-$HOME/.config/pando/tls/ca.crt}"
port=18765
url="${PANDO_URL:-https://localhost:$port}"
work="$(mktemp -d)"
server_pid=""
cleanup() { [ -n "$server_pid" ] && kill "$server_pid" 2>/dev/null || true; rm -rf "$work"; }
trap cleanup EXIT

mkdir -p "$out"
if [ -z "${PANDO_URL:-}" ]; then
  mkdir -p "$work/project" && cd "$work/project" && git init -q
  printf 'hello from the fixture project\n' > hello.txt
  pando serve --port "$port" >"$work/serve.log" 2>&1 &
  server_pid=$!
  for _ in $(seq 1 40); do curl -s --cacert "$ca" "$url/health" >/dev/null 2>&1 && break; sleep 0.5; done
fi

curl_ca=(--cacert "$ca")
token="$(curl -s "${curl_ca[@]}" "$url/api/v1/token" | sed 's/.*"token":"\([^"]*\)".*/\1/')"
[ -n "$token" ] || { echo "no token" >&2; exit 1; }
auth=(-H "X-Pando-Token: $token" -H "X-Pando-Client: pando-assistant-recorder")
scrub() { sed "s/$token/<TOKEN>/g"; }

get()  { curl -s "${curl_ca[@]}" "${auth[@]}" "$url$1" | scrub > "$out/$2"; }
chat() { # name prompt [sessionId]
  local body
  body="$(printf '{"prompt":%s,"sessionId":"%s"}' "$(printf '%s' "$2" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))')" "${3:-}")"
  timeout 180 curl -sN "${curl_ca[@]}" "${auth[@]}" -H 'Content-Type: application/json' -d "$body" \
    "$url/api/v1/chat/stream" | scrub > "$out/$1.sse" || true
}
sid() { sed -n 's/^data: {"sessionId":"\([^"]*\)".*/\1/p' "$out/$1.sse" | head -1; }

curl -s "${curl_ca[@]}" "$url/health" > "$out/health.json"
curl -s "${curl_ca[@]}" "$url/api/v1/token" | sed 's/"token":"[^"]*"/"token":"<TOKEN>"/' > "$out/token.json"
get /api/v1/models models.json
# keep the fixture small: first 4 models only
python3 -I -c 'import json,sys;p=sys.argv[1];d=json.load(open(p));d["models"]=d["models"][:4];json.dump(d,open(p,"w"),indent=1)' "$out/models.json"

chat chat_simple 'Reply with exactly the word: pong. Do not use tools.'
chat chat_tool_read 'Use the read tool to read hello.txt and quote its single line. Nothing else.'
chat chat_slash '/help'
chat chat_goal_status '/goal'

s="$(sid chat_tool_read)"
if [ -n "$s" ]; then
  get "/api/v1/sessions/$s" session_detail.json
  get "/api/v1/sessions/$s/pending" session_pending.json
  timeout 15 curl -sN "${curl_ca[@]}" "${auth[@]}" "$url/api/v1/sessions/$s/stream" | scrub > "$out/session_reattach.sse" || true
fi
get /api/v1/sessions sessions_list.json
get /api/v1/orchestrator/tasks orchestrator_tasks.json
echo "recorded into $out (pando $(sed -n 's/.*"version":"\([^"]*\)".*/\1/p' "$out/health.json"))"
