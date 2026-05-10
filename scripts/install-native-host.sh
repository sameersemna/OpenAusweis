#!/usr/bin/env bash
# install-native-host.sh — Register the OpenAusweis native messaging host for Chromium and Firefox.
#
# Usage:
#   scripts/install-native-host.sh <CHROMIUM_EXTENSION_ID> [BINARY_PATH] [FIREFOX_ADDON_ID]
#
# Arguments:
#   CHROMIUM_EXTENSION_ID
#                  Chrome/Chromium extension ID (from chrome://extensions in developer mode).
#   BINARY_PATH    Optional absolute path to the openausweis-native-host binary.
#                  Defaults to the debug build in target/debug/.
#   FIREFOX_ADDON_ID
#                  Optional Firefox add-on ID used in allowed_extensions.
#                  Example: openausweis@example.org
#
# After running, reload the extension/add-on in the browser and re-open the popup to verify.

set -euo pipefail

CHROMIUM_EXTENSION_ID="${1:-}"
if [[ -z "$CHROMIUM_EXTENSION_ID" ]]; then
  echo "Usage: $0 <CHROMIUM_EXTENSION_ID> [BINARY_PATH] [FIREFOX_ADDON_ID]" >&2
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

FIREFOX_ADDON_ID="${3:-}"

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

FIREFOX_TEMPLATE="$WORKSPACE_ROOT/apps/native-host/org.openausweis.native.firefox.json.template"
if [[ -n "$FIREFOX_ADDON_ID" && ! -f "$FIREFOX_TEMPLATE" ]]; then
  echo "Firefox native host manifest template not found at: $FIREFOX_TEMPLATE" >&2
  exit 1
fi

MANIFEST_JSON=$(
  sed \
    -e "s|__NATIVE_HOST_BINARY_PATH__|${BINARY_PATH}|g" \
    -e "s|__EXTENSION_ID__|${CHROMIUM_EXTENSION_ID}|g" \
    "$TEMPLATE"
)

FIREFOX_MANIFEST_JSON=""
if [[ -n "$FIREFOX_ADDON_ID" ]]; then
  FIREFOX_MANIFEST_JSON=$(
    sed \
      -e "s|__NATIVE_HOST_BINARY_PATH__|${BINARY_PATH}|g" \
      -e "s|__FIREFOX_ADDON_ID__|${FIREFOX_ADDON_ID}|g" \
      "$FIREFOX_TEMPLATE"
  )
fi

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

install_for_firefox_path() {
  local host_dir="$1"
  local browser_name="$2"
  if [[ -z "$FIREFOX_MANIFEST_JSON" ]]; then
    return
  fi

  mkdir -p "$host_dir"
  local dest="$host_dir/org.openausweis.native.json"
  echo "$FIREFOX_MANIFEST_JSON" > "$dest"
  echo "Installed for $browser_name: $dest"
}

install_for_browser "$HOME/.config/google-chrome" "Google Chrome"
install_for_browser "$HOME/.config/chromium"      "Chromium"
install_for_browser "$HOME/.config/chrome-beta"   "Chrome Beta"
install_for_browser "$HOME/.config/chrome-unstable" "Chrome Dev"

if [[ -n "$FIREFOX_ADDON_ID" ]]; then
  install_for_firefox_path "$HOME/.mozilla/native-messaging-hosts" "Firefox"
  install_for_firefox_path "$HOME/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts" "Firefox (Flatpak)"
fi

echo ""
echo "Native messaging host registered."
echo "  Binary : $BINARY_PATH"
echo "  Chromium extension: $CHROMIUM_EXTENSION_ID"
if [[ -n "$FIREFOX_ADDON_ID" ]]; then
  echo "  Firefox add-on: $FIREFOX_ADDON_ID"
fi
echo ""
echo "Next steps:"
echo "  1. Reload the extension in chrome://extensions"
if [[ -n "$FIREFOX_ADDON_ID" ]]; then
  echo "     Reload the Firefox add-on in about:debugging as well."
fi
echo "  2. Ensure the daemon is running: cargo run -p openausweis-daemon"
echo "  3. Open the extension/add-on popup to verify daemon status"
