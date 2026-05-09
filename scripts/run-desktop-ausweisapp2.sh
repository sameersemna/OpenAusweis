#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BRIDGE_PACKAGE="openausweis-ausweisapp2-bridge"
BRIDGE_DEFAULT_BIN="$ROOT_DIR/target/debug/openausweis-ausweisapp2-bridge"

cd "$ROOT_DIR"

# Build the in-repo bridge binary used by daemon ausweisapp2 executor mode.
cargo build -p "$BRIDGE_PACKAGE"

export OPENAUSWEIS_AUTH_EXECUTOR="${OPENAUSWEIS_AUTH_EXECUTOR:-ausweisapp2}"
export OPENAUSWEIS_AUSWEISAPP2_BRIDGE_BIN="${OPENAUSWEIS_AUSWEISAPP2_BRIDGE_BIN:-$BRIDGE_DEFAULT_BIN}"
export OPENAUSWEIS_AA2_STRICT_SUCCESS="${OPENAUSWEIS_AA2_STRICT_SUCCESS:-true}"
export OPENAUSWEIS_AA2_DIAGNOSTICS="${OPENAUSWEIS_AA2_DIAGNOSTICS:-true}"

if [[ -n "${SNAP:-}" || -n "${SNAP_NAME:-}" ]]; then
  npm run --workspace @openausweis/desktop tauri:dev:snap-safe
else
  npm run --workspace @openausweis/desktop tauri:dev
fi
