#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SOCKET_PATH="/tmp/openausweis-daemon.sock"

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

if [[ -n "${SNAP:-}" || -n "${SNAP_NAME:-}" ]]; then
  npm run --workspace @openausweis/desktop tauri:dev:snap-safe
else
  npm run --workspace @openausweis/desktop tauri:dev
fi
