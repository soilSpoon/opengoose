#!/bin/sh
set -eu

BASE_URL="${1:-http://127.0.0.1:8080}"

fetch_page() {
  path="$1"
  curl -sS --max-time 4 "$BASE_URL$path"
}

check_page() {
  path="$1"
  html="$(fetch_page "$path")"
  title="$(printf '%s' "$html" | sed -n 's:.*<title>\([^<]*\)</title>.*:\1:p' | head -n 1)"
  if [ -z "$title" ]; then
    title="untitled"
  fi
  printf '%s ok (%s)\n' "$path" "$title"
}

check_datastar() {
  path="$1"
  html="$(fetch_page "$path")"
  if ! printf '%s' "$html" | grep -q '/assets/vendor/datastar.js'; then
    echo "vendored Datastar asset missing on $path" >&2
    exit 1
  fi
}

check_sse() {
  path="$1"
  headers="$(mktemp)"
  body="$(mktemp)"
  code=0
  curl -sS -N --max-time 2 -D "$headers" "$BASE_URL$path" >"$body" || code=$?
  case "$code" in
    0|28) ;;
    *)
      echo "SSE request failed for $path with code $code" >&2
      rm -f "$headers" "$body"
      exit 1
      ;;
  esac
  if ! grep -qi '^HTTP/.* 200' "$headers"; then
    echo "$path did not return HTTP 200" >&2
    rm -f "$headers" "$body"
    exit 1
  fi
  if ! grep -qi '^content-type: .*text/event-stream' "$headers"; then
    echo "$path did not return text/event-stream" >&2
    rm -f "$headers" "$body"
    exit 1
  fi
  if ! grep -q 'event: datastar-patch-elements' "$body"; then
    echo "$path did not emit an initial datastar patch event" >&2
    rm -f "$headers" "$body"
    exit 1
  fi
  rm -f "$headers" "$body"
  printf '%s sse ok\n' "$path"
}

for path in / /status /sessions /runs /agents /remote-agents /workflows /schedules /triggers /teams /queue; do
  check_page "$path"
done

for path in / /status /remote-agents /sessions; do
  check_datastar "$path"
done

check_sse "/dashboard/events"
check_sse "/status/events"
check_sse "/remote-agents/events"

echo "web smoke checks passed for $BASE_URL"
