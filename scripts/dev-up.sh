#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SOCKET_PATH="/tmp/openausweis-daemon.sock"
DEFAULT_DESKTOP_PORT="1420"

port_in_use() {
  local port="$1"
  ss -ltn "sport = :${port}" | tail -n +2 | grep -q .
}

resolve_desktop_port() {
  local requested="${OPENAUSWEIS_DESKTOP_PORT:-$DEFAULT_DESKTOP_PORT}"

  if ! port_in_use "$requested"; then
    echo "$requested"
    return
  fi

  if [[ -n "${OPENAUSWEIS_DESKTOP_PORT:-}" ]]; then
    echo "Requested desktop dev port ${requested} is already in use. Set OPENAUSWEIS_DESKTOP_PORT to a free port." >&2
    exit 1
  fi

  for candidate in $(seq $((DEFAULT_DESKTOP_PORT + 1)) 1499); do
    if ! port_in_use "$candidate"; then
      echo "Desktop dev port ${DEFAULT_DESKTOP_PORT} is busy; using fallback port ${candidate}." >&2
      echo "$candidate"
      return
    fi
  done

  echo "Unable to find a free desktop dev port between $((DEFAULT_DESKTOP_PORT + 1)) and 1499." >&2
  exit 1
}

should_use_snap_safe() {
  if [[ -n "${SNAP:-}" || -n "${SNAP_NAME:-}" ]]; then
    return 0
  fi

  if [[ "${LD_LIBRARY_PATH:-}" == *"/snap/"* ]]; then
    return 0
  fi

  if [[ "${LOCPATH:-}" == *"/snap/"* ]]; then
    return 0
  fi

  return 1
}

cd "$ROOT_DIR"

cargo run -p openausweis-daemon &
DAEMON_PID=$!

cleanup() {
  kill "$DAEMON_PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# Wait for daemon socket to appear before launching the desktop shell.
for _ in $(seq 1 40); do
  if [[ -S "$SOCKET_PATH" ]]; then
    break
  fi
  sleep 0.25
done

if [[ ! -S "$SOCKET_PATH" ]]; then
  echo "daemon socket did not appear at $SOCKET_PATH" >&2
  exit 1
fi

DESKTOP_PORT="$(resolve_desktop_port)"
export OPENAUSWEIS_DESKTOP_PORT="$DESKTOP_PORT"

if should_use_snap_safe; then
  npm run --workspace @openausweis/desktop tauri:dev:snap-safe -- --port "$DESKTOP_PORT"
else
  npm run --workspace @openausweis/desktop tauri:dev -- --port "$DESKTOP_PORT"
fi
