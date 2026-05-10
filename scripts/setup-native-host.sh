#!/usr/bin/env bash
# setup-native-host.sh — Install native-host manifests and run packaging validation.
#
# Usage examples:
#   scripts/setup-native-host.sh --chromium-id abcdefghijklmnopqrstuvwxyzabcdef
#   scripts/setup-native-host.sh --firefox-id openausweis@example.org
#   scripts/setup-native-host.sh --chromium-id <ID> --firefox-id <ADDON_ID>
#   scripts/setup-native-host.sh --chromium-id <ID> --binary ./target/debug/openausweis-native-host
#
# Optional flags:
#   --skip-validate     Skip npm run validate:linux-packaging after install.
#   --help              Show usage.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CHROMIUM_ID=""
FIREFOX_ID=""
BINARY_PATH=""
SKIP_VALIDATE=0

print_usage() {
  cat <<'USAGE'
Usage:
  scripts/setup-native-host.sh [--chromium-id ID] [--firefox-id ID] [--binary PATH] [--skip-validate]

Arguments:
  --chromium-id ID   Chromium extension ID used for allowed_origins manifests.
  --firefox-id ID    Firefox add-on ID used for allowed_extensions manifests.
  --binary PATH      Optional native host binary path.
  --skip-validate    Skip packaging validation step.
  --help             Print this help message.

At least one of --chromium-id or --firefox-id must be provided.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --chromium-id)
      CHROMIUM_ID="${2:-}"
      shift 2
      ;;
    --firefox-id)
      FIREFOX_ID="${2:-}"
      shift 2
      ;;
    --binary)
      BINARY_PATH="${2:-}"
      shift 2
      ;;
    --skip-validate)
      SKIP_VALIDATE=1
      shift
      ;;
    --help|-h)
      print_usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      print_usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$CHROMIUM_ID" && -z "$FIREFOX_ID" ]]; then
  echo "Provide at least one target browser identifier." >&2
  print_usage >&2
  exit 1
fi

if [[ -z "$BINARY_PATH" ]]; then
  BINARY_PATH="$WORKSPACE_ROOT/target/debug/openausweis-native-host"
fi

if [[ ! -f "$BINARY_PATH" ]]; then
  echo "Native host binary not found at: $BINARY_PATH" >&2
  echo "Build it first with: cargo build -p openausweis-native-host" >&2
  exit 1
fi

if [[ -n "$CHROMIUM_ID" ]]; then
  echo "Installing native host manifest for Chromium targets..."
  if [[ -n "$FIREFOX_ID" ]]; then
    "$SCRIPT_DIR/install-native-host.sh" "$CHROMIUM_ID" "$BINARY_PATH" "$FIREFOX_ID"
  else
    "$SCRIPT_DIR/install-native-host.sh" "$CHROMIUM_ID" "$BINARY_PATH"
  fi
elif [[ -n "$FIREFOX_ID" ]]; then
  echo "Installing native host manifest for Firefox targets..."
  "$SCRIPT_DIR/install-native-host-firefox.sh" "$FIREFOX_ID" "$BINARY_PATH"
fi

if (( SKIP_VALIDATE == 0 )); then
  echo ""
  echo "Running packaging validation..."
  validate_cmd=("$SCRIPT_DIR/validate-linux-packaging.sh")
  if [[ -n "$CHROMIUM_ID" ]]; then
    validate_cmd+=("--expect-chromium-id" "$CHROMIUM_ID")
  fi
  if [[ -n "$FIREFOX_ID" ]]; then
    validate_cmd+=("--expect-firefox-id" "$FIREFOX_ID")
  fi

  (
    cd "$WORKSPACE_ROOT"
    "${validate_cmd[@]}"
  )
fi

echo ""
echo "Native host setup workflow completed."
