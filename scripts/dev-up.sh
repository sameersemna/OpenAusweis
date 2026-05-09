#!/usr/bin/env bash
set -euo pipefail

cargo run -p openausweis-daemon &
DAEMON_PID=$!

cleanup() {
  kill "$DAEMON_PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT

npm run dev --workspace @openausweis/desktop
