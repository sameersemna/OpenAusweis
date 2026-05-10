#!/usr/bin/env bash
# install-native-host-firefox.sh — Register the OpenAusweis native messaging host for Firefox.
#
# Usage:
#   scripts/install-native-host-firefox.sh <FIREFOX_ADDON_ID> [BINARY_PATH]
#
# Arguments:
#   FIREFOX_ADDON_ID
#                  Firefox add-on ID used in allowed_extensions.
#                  Example: openausweis@example.org
#   BINARY_PATH    Optional absolute path to the openausweis-native-host binary.
#                  Defaults to the debug build in target/debug/.

set -euo pipefail

FIREFOX_ADDON_ID="${1:-}"
if [[ -z "$FIREFOX_ADDON_ID" ]]; then
  echo "Usage: $0 <FIREFOX_ADDON_ID> [BINARY_PATH]" >&2
  echo "" >&2
  echo "Find your add-on ID in about:debugging -> This Firefox." >&2
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

FIREFOX_TEMPLATE="$WORKSPACE_ROOT/apps/native-host/org.openausweis.native.firefox.json.template"
if [[ ! -f "$FIREFOX_TEMPLATE" ]]; then
  echo "Firefox native host manifest template not found at: $FIREFOX_TEMPLATE" >&2
  exit 1
fi

FIREFOX_MANIFEST_JSON=$(
  sed \
    -e "s|__NATIVE_HOST_BINARY_PATH__|${BINARY_PATH}|g" \
    -e "s|__FIREFOX_ADDON_ID__|${FIREFOX_ADDON_ID}|g" \
    "$FIREFOX_TEMPLATE"
)

install_for_firefox_path() {
  local host_dir="$1"
  local browser_name="$2"
  mkdir -p "$host_dir"
  local dest="$host_dir/org.openausweis.native.json"
  echo "$FIREFOX_MANIFEST_JSON" > "$dest"
  echo "Installed for $browser_name: $dest"
}

install_for_firefox_path "$HOME/.mozilla/native-messaging-hosts" "Firefox"
install_for_firefox_path "$HOME/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts" "Firefox (Flatpak)"

echo ""
echo "Firefox native messaging host registered."
echo "  Binary : $BINARY_PATH"
echo "  Firefox add-on: $FIREFOX_ADDON_ID"
echo ""
echo "Next steps:"
echo "  1. Reload the add-on in about:debugging"
echo "  2. Ensure the daemon is running: cargo run -p openausweis-daemon"
echo "  3. Open the add-on popup to verify daemon status"
