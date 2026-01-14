#!/usr/bin/env bash
set -euo pipefail

API_URL="${API_URL:-http://127.0.0.1:3000}"
BIN="${BIN:-cargo run --bin api-finalized}"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

require_cmd curl
require_cmd jq

start_server() {
  echo "Starting server: $BIN" >&2
  # Run in background and wait for /status.
  (cd "$(dirname "$0")/.." && ${BIN} >/tmp/atomiq_api_finalized.log 2>&1) &
  SERVER_PID=$!

  for _ in $(seq 1 80); do
    if curl -fsS "$API_URL/status" >/dev/null 2>&1; then
      echo "Server is up (pid=$SERVER_PID)" >&2
      return 0
    fi
    sleep 0.1
  done

  echo "Server did not become ready. Logs:" >&2
  tail -n 200 /tmp/atomiq_api_finalized.log >&2 || true
  kill "$SERVER_PID" >/dev/null 2>&1 || true
  exit 1
}

stop_server() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    echo "Stopping server (pid=$SERVER_PID)" >&2
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
    unset SERVER_PID
  fi
}

play_and_get_pubkey() {
  # POST /api/coinflip/play returns a finalized response.
  # Extract VRF public key from the result VRF bundle.
  local resp
  resp=$(curl -fsS -X POST "$API_URL/api/coinflip/play" \
    -H 'content-type: application/json' \
    -d '{"player_id":"restart-test","choice":"heads","token":{"symbol":"SOL"},"bet_amount":0.000000001}')
  echo "$resp" | jq -r '.result.vrf.public_key'
}

cleanup() {
  stop_server
}
trap cleanup EXIT

start_server
key1=$(play_and_get_pubkey)
if [[ -z "$key1" || "$key1" == "null" ]]; then
  echo "Failed to fetch VRF public key from first run" >&2
  exit 1
fi

echo "First run VRF pubkey: $key1" >&2

stop_server
start_server
key2=$(play_and_get_pubkey)
if [[ -z "$key2" || "$key2" == "null" ]]; then
  echo "Failed to fetch VRF public key from second run" >&2
  exit 1
fi

echo "Second run VRF pubkey: $key2" >&2

if [[ "$key1" != "$key2" ]]; then
  echo "FAIL: VRF public key changed across restart" >&2
  exit 1
fi

echo "OK: VRF public key stable across restart" >&2
