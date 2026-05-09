#!/usr/bin/env bash
# install-native-host.sh — Register the OpenAusweis native messaging host with Chrome/Chromium.
#
# Usage:
#   scripts/install-native-host.sh <EXTENSION_ID> [BINARY_PATH]
#
# Arguments:
#   EXTENSION_ID   Chrome extension ID (visible in chrome://extensions in developer mode).
#   BINARY_PATH    Optional absolute path to the openausweis-native-host binary.
#                  Defaults to the debug build in target/debug/.
#
# After running, reload the extension in Chrome and re-open the extension popup to verify.

set -euo pipefail

EXTENSION_ID="${1:-}"
if [[ -z "$EXTENSION_ID" ]]; then
  echo "Usage: $0 <EXTENSION_ID> [BINARY_PATH]" >&2
  echo "" >&2
  echo "Find your extension ID at chrome://extensions (enable Developer mode)." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BINARY_PATH="${2:-}"
if [[ -z "$BINARY_PATH" ]]; then
  BINARY_PATH="$WORKSPACE_ROOT/target/debug/openausweis-native-host"
fi

if [[ ! -f "$BINARY_PATH" ]]; then
  echo "Native host binary not found at: $BINARY_PATH" >&2
  echo "Build it first with: cargo build -p openausweis-native-host" >&2
  exit 1
fi

TEMPLATE="$WORKSPACE_ROOT/apps/native-host/org.openausweis.native.json.template"
if [[ ! -f "$TEMPLATE" ]]; then
  echo "Native host manifest template not found at: $TEMPLATE" >&2
  exit 1
fi

MANIFEST_JSON=$(
  sed \
    -e "s|__NATIVE_HOST_BINARY_PATH__|${BINARY_PATH}|g" \
    -e "s|__EXTENSION_ID__|${EXTENSION_ID}|g" \
    "$TEMPLATE"
)

install_for_browser() {
  local config_dir="$1"
  local browser_name="$2"
  if [[ ! -d "$config_dir" ]]; then
    return
  fi
  local host_dir="$config_dir/NativeMessagingHosts"
  mkdir -p "$host_dir"
  local dest="$host_dir/org.openausweis.native.json"
  echo "$MANIFEST_JSON" > "$dest"
  echo "Installed for $browser_name: $dest"
}

install_for_browser "$HOME/.config/google-chrome" "Google Chrome"
install_for_browser "$HOME/.config/chromium"      "Chromium"
install_for_browser "$HOME/.config/chrome-beta"   "Chrome Beta"
install_for_browser "$HOME/.config/chrome-unstable" "Chrome Dev"

echo ""
echo "Native messaging host registered."
echo "  Binary : $BINARY_PATH"
echo "  Extension: $EXTENSION_ID"
echo ""
echo "Next steps:"
echo "  1. Reload the extension in chrome://extensions"
echo "  2. Ensure the daemon is running: cargo run -p openausweis-daemon"
echo "  3. Open the extension popup to verify the daemon status"
