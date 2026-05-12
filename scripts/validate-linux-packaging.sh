#!/usr/bin/env bash
# validate-linux-packaging.sh
#
# Runs Linux packaging/runtime readiness checks for OpenAusweis on Ubuntu 24+.
# Focus areas:
# - Ubuntu version baseline
# - GNOME/Wayland compatibility hints
# - Daemon socket and native-host install paths
# - Chromium/Firefox native messaging manifest validation
# - Optional strict ID drift checks for installed manifests
# - Snap/Flatpak availability and interface visibility
# - pcscd runtime availability

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

pass_count=0
warn_count=0
fail_count=0
ACTIONABLE_FIXES=()
EXPECTED_CHROMIUM_ID="${OPENAUSWEIS_EXPECTED_CHROMIUM_EXTENSION_ID:-}"
EXPECTED_FIREFOX_ID="${OPENAUSWEIS_EXPECTED_FIREFOX_ADDON_ID:-}"

log_pass() {
  pass_count=$((pass_count + 1))
  echo "[PASS] $1"
}

log_warn() {
  warn_count=$((warn_count + 1))
  echo "[WARN] $1"
}

log_warn_fix() {
  local message="$1"
  local fix="$2"
  log_warn "$message"
  ACTIONABLE_FIXES+=("$fix")
}

log_fail() {
  fail_count=$((fail_count + 1))
  echo "[FAIL] $1"
}

require_cmd() {
  local cmd="$1"
  local desc="$2"
  if command -v "$cmd" >/dev/null 2>&1; then
    log_pass "$desc available ($cmd)"
    return 0
  fi

  log_fail "$desc missing ($cmd)"
  return 1
}

resolve_daemon_socket_path() {
  if [[ -n "${OPENAUSWEIS_DAEMON_SOCKET:-}" ]]; then
    echo "$OPENAUSWEIS_DAEMON_SOCKET"
    return
  fi

  if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
    echo "$XDG_RUNTIME_DIR/openausweis/daemon.sock"
    return
  fi

  echo "/tmp/openausweis-daemon.sock"
}

check_os_baseline() {
  if [[ "$(uname -s)" != "Linux" ]]; then
    log_fail "Host OS is not Linux"
    return
  fi

  log_pass "Host OS is Linux"

  if [[ ! -f /etc/os-release ]]; then
    log_warn "/etc/os-release not found; cannot verify Ubuntu baseline"
    return
  fi

  # shellcheck disable=SC1091
  source /etc/os-release

  if [[ "${ID:-}" != "ubuntu" ]]; then
    log_warn "Distro is ${ID:-unknown}; target baseline is Ubuntu 24+"
    return
  fi

  local version="${VERSION_ID:-0}"
  local major="${version%%.*}"
  if [[ "$major" =~ ^[0-9]+$ ]] && (( major >= 24 )); then
    log_pass "Ubuntu baseline satisfied (${VERSION_ID})"
  else
    log_fail "Ubuntu baseline not satisfied (${VERSION_ID:-unknown}); require 24+"
  fi
}

check_desktop_session() {
  local desktop="${XDG_CURRENT_DESKTOP:-unknown}"
  local desktop_family="unknown"

  if [[ "$desktop" == *GNOME* ]]; then
    desktop_family="gnome"
    log_pass "GNOME desktop detected (${desktop})"
  elif [[ "$desktop" == *KDE* || "$desktop" == *Plasma* ]]; then
    desktop_family="kde"
    log_pass "KDE Plasma desktop detected (${desktop})"
  else
    log_warn "Unrecognized desktop detected (${desktop}); tray behavior may vary"
  fi

  local session_type="${XDG_SESSION_TYPE:-unknown}"
  if [[ "$session_type" == "wayland" ]]; then
    log_pass "Wayland session detected"
    case "$desktop_family" in
      gnome)
        log_warn_fix \
          "GNOME Wayland is the primary compatibility target; tray and focus behavior should be validated explicitly" \
          "Run a GNOME Wayland validation pass and execute: npm run validate:desktop-polish"
        ;;
      kde)
        log_warn_fix \
          "KDE Wayland detected; validate tray visibility, attention requests, and browser handoff in Plasma" \
          "Run a KDE Wayland validation pass and execute: npm run validate:desktop-polish"
        ;;
      *)
        log_warn_fix \
          "Wayland session detected with an unrecognized desktop shell; validate tray and focus behavior explicitly" \
          "Run the desktop polish checklist in the exact target desktop session"
        ;;
    esac
  elif [[ "$session_type" == "x11" ]]; then
    log_warn_fix \
      "X11 session detected; Wayland compatibility not exercised" \
      "Run a Wayland session validation pass and execute: npm run validate:linux-packaging"
  else
    log_warn_fix \
      "Unknown session type (${session_type}); cannot confirm Wayland coverage" \
      "Set XDG_SESSION_TYPE and run validation in the target desktop session"
  fi
}

print_usage() {
  cat <<'USAGE'
Usage:
  scripts/validate-linux-packaging.sh [--expect-chromium-id ID] [--expect-firefox-id ID]

Options:
  --expect-chromium-id ID   Validate installed Chromium allowed_origins include this extension ID.
  --expect-firefox-id ID    Validate installed Firefox allowed_extensions include this add-on ID.
  --help                    Print this help message.

Environment fallback:
  OPENAUSWEIS_EXPECTED_CHROMIUM_EXTENSION_ID
  OPENAUSWEIS_EXPECTED_FIREFOX_ADDON_ID
USAGE
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --expect-chromium-id)
        EXPECTED_CHROMIUM_ID="${2:-}"
        shift 2
        ;;
      --expect-firefox-id)
        EXPECTED_FIREFOX_ID="${2:-}"
        shift 2
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
}

extract_json_array_values() {
  local file="$1"
  local key="$2"

  awk -v key="$key" '
    index($0, "\"" key "\"") { in_array=1; next }
    in_array {
      if ($0 ~ /\]/) { in_array=0; exit }
      line = $0
      while (match(line, /"[^"]+"/)) {
        value = substr(line, RSTART + 1, RLENGTH - 2)
        print value
        line = substr(line, RSTART + RLENGTH)
      }
    }
  ' "$file"
}

verify_chromium_manifest_id() {
  local manifest="$1"
  local expected_id="$2"
  local found_match=0

  while IFS= read -r origin; do
    if [[ "$origin" =~ ^chrome-extension://([^/]+)/?$ ]]; then
      local found_id="${BASH_REMATCH[1]}"
      if [[ "$found_id" == "$expected_id" ]]; then
        found_match=1
      fi
    fi
  done < <(extract_json_array_values "$manifest" "allowed_origins")

  if (( found_match == 1 )); then
    log_pass "Chromium manifest ID matches expected value in $manifest"
  else
    log_fail "Chromium manifest ID drift in $manifest (expected: $expected_id)"
  fi
}

verify_firefox_manifest_id() {
  local manifest="$1"
  local expected_id="$2"
  local found_match=0

  while IFS= read -r addon_id; do
    if [[ "$addon_id" == "$expected_id" ]]; then
      found_match=1
    fi
  done < <(extract_json_array_values "$manifest" "allowed_extensions")

  if (( found_match == 1 )); then
    log_pass "Firefox manifest ID matches expected value in $manifest"
  else
    log_fail "Firefox manifest ID drift in $manifest (expected: $expected_id)"
  fi
}

check_native_host_manifest() {
  local -a chromium_paths=(
    "$HOME/.config/google-chrome/NativeMessagingHosts/org.openausweis.native.json"
    "$HOME/.config/chromium/NativeMessagingHosts/org.openausweis.native.json"
    "$HOME/.config/chrome-beta/NativeMessagingHosts/org.openausweis.native.json"
    "$HOME/.config/chrome-unstable/NativeMessagingHosts/org.openausweis.native.json"
  )

  local -a firefox_paths=(
    "$HOME/.mozilla/native-messaging-hosts/org.openausweis.native.json"
    "$HOME/snap/firefox/common/.mozilla/native-messaging-hosts/org.openausweis.native.json"
    "$HOME/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/org.openausweis.native.json"
  )

  local found_any=0
  local found_chromium=0
  local found_firefox=0

  for manifest in "${chromium_paths[@]}"; do
    if [[ -f "$manifest" ]]; then
      found_any=1
      found_chromium=1
      log_pass "Chromium native host manifest found: $manifest"

      local binary_path
      binary_path="$(sed -n 's/^[[:space:]]*"path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest" | head -n1)"
      if [[ -z "$binary_path" ]]; then
        log_warn "Could not parse binary path from manifest: $manifest"
      elif [[ -x "$binary_path" ]]; then
        log_pass "Native host binary is executable: $binary_path"
      elif [[ -f "$binary_path" ]]; then
        log_fail "Native host binary exists but is not executable: $binary_path"
      else
        log_fail "Native host binary path missing: $binary_path"
      fi

      if grep -q '"allowed_origins"' "$manifest"; then
        log_pass "Chromium manifest contains allowed_origins"
        if [[ -n "$EXPECTED_CHROMIUM_ID" ]]; then
          verify_chromium_manifest_id "$manifest" "$EXPECTED_CHROMIUM_ID"
        fi
      else
        log_fail "Chromium manifest missing allowed_origins: $manifest"
      fi
    fi
  done

  for manifest in "${firefox_paths[@]}"; do
    if [[ -f "$manifest" ]]; then
      found_any=1
      found_firefox=1
      log_pass "Firefox native host manifest found: $manifest"

      local binary_path
      binary_path="$(sed -n 's/^[[:space:]]*"path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest" | head -n1)"
      if [[ -z "$binary_path" ]]; then
        log_warn "Could not parse binary path from manifest: $manifest"
      elif [[ -x "$binary_path" ]]; then
        log_pass "Native host binary is executable: $binary_path"
      elif [[ -f "$binary_path" ]]; then
        log_fail "Native host binary exists but is not executable: $binary_path"
      else
        log_fail "Native host binary path missing: $binary_path"
      fi

      if grep -q '"allowed_extensions"' "$manifest"; then
        log_pass "Firefox manifest contains allowed_extensions"
        if [[ -n "$EXPECTED_FIREFOX_ID" ]]; then
          verify_firefox_manifest_id "$manifest" "$EXPECTED_FIREFOX_ID"
        fi
      else
        log_warn "Firefox manifest missing allowed_extensions (may be Chromium format): $manifest"
      fi
    fi
  done

  if (( found_any == 0 )); then
    log_warn_fix \
      "No native messaging manifest found in Chromium/Firefox default locations" \
      "Run one-step setup: scripts/setup-native-host.sh --chromium-id <CHROMIUM_EXTENSION_ID> [--firefox-id <FIREFOX_ADDON_ID>] [--binary PATH]"
  fi

  if [[ -n "$EXPECTED_CHROMIUM_ID" && $found_chromium -eq 0 ]]; then
    log_fail "Expected Chromium manifest ID provided, but no Chromium native host manifest was found"
  fi

  if [[ -n "$EXPECTED_FIREFOX_ID" && $found_firefox -eq 0 ]]; then
    log_fail "Expected Firefox add-on ID provided, but no Firefox native host manifest was found"
  fi
}

check_browser_runtime_paths() {
  local -a browsers=(
    "firefox:Firefox:$HOME/.mozilla/native-messaging-hosts/org.openausweis.native.json:$HOME/snap/firefox/common/.mozilla/native-messaging-hosts/org.openausweis.native.json:$HOME/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/org.openausweis.native.json"
    "chromium:Chromium:$HOME/.config/chromium/NativeMessagingHosts/org.openausweis.native.json"
    "google-chrome:Google Chrome:$HOME/.config/google-chrome/NativeMessagingHosts/org.openausweis.native.json"
    "brave-browser:Brave:$HOME/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/org.openausweis.native.json"
  )

  for browser_spec in "${browsers[@]}"; do
    IFS=: read -r command name primary_manifest fallback_manifest <<< "$browser_spec"
    if command -v "$command" >/dev/null 2>&1; then
      log_pass "$name command available ($command)"

      if [[ -f "$primary_manifest" ]]; then
        log_pass "$name native messaging manifest present: $primary_manifest"
      elif [[ -n "${fallback_manifest:-}" && -f "$fallback_manifest" ]]; then
        log_pass "$name native messaging manifest present: $fallback_manifest"
      else
        log_warn_fix \
          "$name command is present but no native messaging manifest was found in the expected locations" \
          "Run scripts/setup-native-host.sh with the browser ID and re-run validation"
      fi
    else
      log_warn "$name command not found; cannot validate browser/runtime integration for this browser"
    fi
  done
}

check_daemon_socket_layout() {
  local socket_path
  socket_path="$(resolve_daemon_socket_path)"
  local parent
  parent="$(dirname "$socket_path")"

  if [[ -d "$parent" ]]; then
    log_pass "Daemon socket parent directory exists: $parent"
    local perms
    perms="$(stat -c '%a' "$parent" 2>/dev/null || echo "")"
    if [[ -n "$perms" && "$perms" == "700" ]]; then
      log_pass "Daemon socket parent permissions are 700"
    else
      log_warn "Daemon socket parent permissions are ${perms:-unknown}; expected 700"
    fi
  else
    log_warn_fix \
      "Daemon socket parent directory does not exist yet: $parent" \
      "Start daemon once: cargo run -p openausweis-daemon"
  fi

  if [[ -S "$socket_path" ]]; then
    log_pass "Daemon socket exists: $socket_path"
    local socket_perms
    socket_perms="$(stat -c '%a' "$socket_path" 2>/dev/null || echo "")"
    if [[ -n "$socket_perms" && "$socket_perms" == "600" ]]; then
      log_pass "Daemon socket permissions are 600"
    else
      log_warn_fix \
        "Daemon socket permissions are ${socket_perms:-unknown}; expected 600" \
        "Restart daemon from current build so it recreates socket with hardened permissions"
    fi
  else
    log_warn_fix \
      "Daemon socket not present (daemon may be stopped): $socket_path" \
      "Start daemon and rerun validation: cargo run -p openausweis-daemon"
  fi
}

check_smartcard_runtime() {
  if ! command -v systemctl >/dev/null 2>&1; then
    log_warn "systemctl not available; cannot verify pcscd service"
    return
  fi

  if systemctl is-active --quiet pcscd; then
    log_pass "pcscd service is active"
  else
    log_warn_fix \
      "pcscd service is not active" \
      "Install/enable smartcard stack: sudo apt install -y pcscd libpcsclite1 libpcsclite-dev pkgconf pcsc-tools && sudo systemctl enable --now pcscd"
  fi
}

check_snap_flatpak() {
  if command -v snap >/dev/null 2>&1; then
    log_pass "snap command available"
    if snap list openausweis >/dev/null 2>&1; then
      log_pass "OpenAusweis snap is installed"
      local connections
      connections="$(snap connections openausweis 2>/dev/null || true)"
      if [[ "$connections" == *"raw-usb"* || "$connections" == *"hardware-observe"* ]]; then
        log_pass "Snap connections include smartcard-relevant interfaces"
      else
        log_warn "OpenAusweis snap connections missing expected smartcard interfaces"
      fi
    else
      log_warn_fix \
        "OpenAusweis snap is not installed" \
        "Build/install snap once packaging is wired: snapcraft && sudo snap install --dangerous ./openausweis_*.snap"
    fi
  else
    log_warn_fix "snap command unavailable" "Install snapd: sudo apt install -y snapd"
  fi

  if command -v flatpak >/dev/null 2>&1; then
    log_pass "flatpak command available"
  else
    log_warn_fix "flatpak command unavailable" "Install Flatpak tooling: sudo apt install -y flatpak"
  fi
}

check_snapcraft_expectations() {
  local -a candidates=(
    "$WORKSPACE_ROOT/snapcraft.yaml"
    "$WORKSPACE_ROOT/snap/snapcraft.yaml"
  )

  local snapcraft_file=""
  for candidate in "${candidates[@]}"; do
    if [[ -f "$candidate" ]]; then
      snapcraft_file="$candidate"
      break
    fi
  done

  if [[ -z "$snapcraft_file" ]]; then
    log_warn_fix \
      "No snapcraft.yaml found; cannot verify expected snap interface declarations" \
      "Create snap/snapcraft.yaml scaffold and rerun validation"
    return
  fi

  log_pass "Found snapcraft config: $snapcraft_file"

  local -a expected_plugs=(
    "raw-usb"
    "hardware-observe"
    "desktop"
    "desktop-legacy"
    "wayland"
  )

  for plug in "${expected_plugs[@]}"; do
    if grep -Eiq "(^|[[:space:]-])${plug}([[:space:]]|$|:)" "$snapcraft_file"; then
      log_pass "snapcraft declares expected interface hint: $plug"
    else
      log_warn "snapcraft may be missing expected interface hint: $plug"
    fi
  done
}

print_actionable_fixes() {
  if (( ${#ACTIONABLE_FIXES[@]} == 0 )); then
    return
  fi

  echo ""
  echo "Actionable fixes:"
  local idx=1
  for fix in "${ACTIONABLE_FIXES[@]}"; do
    echo "  $idx. $fix"
    idx=$((idx + 1))
  done
}

main() {
  parse_args "$@"

  echo "OpenAusweis Linux Packaging Validation"
  echo "Workspace: $WORKSPACE_ROOT"
  if [[ -n "$EXPECTED_CHROMIUM_ID" ]]; then
    echo "Expected Chromium ID: $EXPECTED_CHROMIUM_ID"
  fi
  if [[ -n "$EXPECTED_FIREFOX_ID" ]]; then
    echo "Expected Firefox ID: $EXPECTED_FIREFOX_ID"
  fi
  echo ""

  require_cmd cargo "Rust toolchain" || true
  require_cmd npm "Node.js package manager" || true

  check_os_baseline
  check_desktop_session
  check_native_host_manifest
  check_browser_runtime_paths
  check_daemon_socket_layout
  check_smartcard_runtime
  check_snap_flatpak
  check_snapcraft_expectations

  echo ""
  echo "Summary: ${pass_count} pass, ${warn_count} warn, ${fail_count} fail"
  print_actionable_fixes

  if (( fail_count > 0 )); then
    exit 1
  fi
}

main "$@"
