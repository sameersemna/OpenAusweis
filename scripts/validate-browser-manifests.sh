#!/usr/bin/env bash
# validate-browser-manifests.sh
#
# Validates browser native messaging manifest setup for OpenAusweis.
# Checks:
# - Manifest existence and readability
# - JSON syntax validity
# - Required fields presence
# - Binary path accessibility
# - Optional: expected extension/add-on IDs

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

pass_count=0
warn_count=0
fail_count=0

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

log_fail() {
  fail_count=$((fail_count + 1))
  echo "[FAIL] $1"
}

validate_json() {
  local file="$1"
  if ! jq empty "$file" 2>/dev/null; then
    return 1
  fi
  return 0
}

extract_json_value() {
  local file="$1"
  local key="$2"
  jq -r ".$key // empty" "$file" 2>/dev/null || echo ""
}

extract_json_array() {
  local file="$1"
  local key="$2"
  jq -r ".${key}[]? // empty" "$file" 2>/dev/null || echo ""
}

check_chromium_manifest() {
  local -a paths=(
    "$HOME/.config/google-chrome/NativeMessagingHosts/org.openausweis.native.json"
    "$HOME/.config/chromium/NativeMessagingHosts/org.openausweis.native.json"
    "$HOME/.config/chrome-beta/NativeMessagingHosts/org.openausweis.native.json"
    "$HOME/.config/chrome-unstable/NativeMessagingHosts/org.openausweis.native.json"
  )

  local found=0
  for manifest in "${paths[@]}"; do
    if [[ ! -f "$manifest" ]]; then
      continue
    fi

    found=1
    log_pass "Chromium manifest found: $manifest"

    if ! validate_json "$manifest"; then
      log_fail "Chromium manifest JSON syntax error: $manifest"
      continue
    fi

    log_pass "Chromium manifest JSON is valid: $manifest"

    local binary_path
    binary_path="$(extract_json_value "$manifest" "path")"
    if [[ -z "$binary_path" ]]; then
      log_fail "Chromium manifest missing 'path' field: $manifest"
      continue
    fi

    if [[ -x "$binary_path" ]]; then
      log_pass "Chromium native host binary is executable: $binary_path"
    elif [[ -f "$binary_path" ]]; then
      log_fail "Chromium native host binary exists but is not executable: $binary_path"
    else
      log_fail "Chromium native host binary path missing: $binary_path"
    fi

    local type_value
    type_value="$(extract_json_value "$manifest" "type")"
    if [[ "$type_value" == "stdio" ]]; then
      log_pass "Chromium manifest type is 'stdio'"
    else
      log_warn "Chromium manifest type is '${type_value:-unknown}'; expected 'stdio'"
    fi

    local has_origins=0
    if extract_json_array "$manifest" "allowed_origins" | grep -q .; then
      has_origins=1
      log_pass "Chromium manifest contains allowed_origins"

      if [[ -n "$EXPECTED_CHROMIUM_ID" ]]; then
        local found_id=0
        while IFS= read -r origin; do
          if [[ "$origin" == *"$EXPECTED_CHROMIUM_ID"* ]]; then
            found_id=1
            log_pass "Chromium manifest includes expected extension ID: $EXPECTED_CHROMIUM_ID"
            break
          fi
        done < <(extract_json_array "$manifest" "allowed_origins")

        if [[ $found_id -eq 0 ]]; then
          log_warn "Chromium manifest does not include expected extension ID: $EXPECTED_CHROMIUM_ID"
        fi
      fi
    else
      log_fail "Chromium manifest missing allowed_origins field: $manifest"
    fi

    break
  done

  if [[ $found -eq 0 ]]; then
    log_warn "No Chromium native messaging manifest found in standard locations"
  fi
}

check_firefox_manifest() {
  local -a paths=(
    "$HOME/.mozilla/native-messaging-hosts/org.openausweis.native.json"
    "$HOME/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/org.openausweis.native.json"
  )

  local found=0
  for manifest in "${paths[@]}"; do
    if [[ ! -f "$manifest" ]]; then
      continue
    fi

    found=1
    log_pass "Firefox manifest found: $manifest"

    if ! validate_json "$manifest"; then
      log_fail "Firefox manifest JSON syntax error: $manifest"
      continue
    fi

    log_pass "Firefox manifest JSON is valid: $manifest"

    local binary_path
    binary_path="$(extract_json_value "$manifest" "path")"
    if [[ -z "$binary_path" ]]; then
      log_fail "Firefox manifest missing 'path' field: $manifest"
      continue
    fi

    if [[ -x "$binary_path" ]]; then
      log_pass "Firefox native host binary is executable: $binary_path"
    elif [[ -f "$binary_path" ]]; then
      log_fail "Firefox native host binary exists but is not executable: $binary_path"
    else
      log_fail "Firefox native host binary path missing: $binary_path"
    fi

    local type_value
    type_value="$(extract_json_value "$manifest" "type")"
    if [[ "$type_value" == "stdio" ]]; then
      log_pass "Firefox manifest type is 'stdio'"
    else
      log_warn "Firefox manifest type is '${type_value:-unknown}'; expected 'stdio'"
    fi

    # Firefox can use either allowed_extensions or allowed_origins
    local has_extensions=0
    local has_origins=0

    if extract_json_array "$manifest" "allowed_extensions" | grep -q .; then
      has_extensions=1
      log_pass "Firefox manifest contains allowed_extensions"

      if [[ -n "$EXPECTED_FIREFOX_ID" ]]; then
        local found_id=0
        while IFS= read -r addon_id; do
          if [[ "$addon_id" == "$EXPECTED_FIREFOX_ID" ]]; then
            found_id=1
            log_pass "Firefox manifest includes expected add-on ID: $EXPECTED_FIREFOX_ID"
            break
          fi
        done < <(extract_json_array "$manifest" "allowed_extensions")

        if [[ $found_id -eq 0 ]]; then
          log_warn "Firefox manifest does not include expected add-on ID: $EXPECTED_FIREFOX_ID"
        fi
      fi
    fi

    if extract_json_array "$manifest" "allowed_origins" | grep -q .; then
      has_origins=1
      log_pass "Firefox manifest contains allowed_origins (Chromium-format fallback)"
    fi

    if [[ $has_extensions -eq 0 && $has_origins -eq 0 ]]; then
      log_fail "Firefox manifest missing both allowed_extensions and allowed_origins: $manifest"
    fi

    break
  done

  if [[ $found -eq 0 ]]; then
    log_warn "No Firefox native messaging manifest found in standard locations"
  fi
}

print_usage() {
  cat <<'USAGE'
Usage:
  scripts/validate-browser-manifests.sh [--expect-chromium-id ID] [--expect-firefox-id ID]

Options:
  --expect-chromium-id ID   Validate installed Chromium manifest includes this extension ID.
  --expect-firefox-id ID    Validate installed Firefox manifest includes this add-on ID.
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

main() {
  parse_args "$@"

  echo "OpenAusweis Browser Native Messaging Manifest Validation"
  if [[ -n "$EXPECTED_CHROMIUM_ID" ]]; then
    echo "Expected Chromium ID: $EXPECTED_CHROMIUM_ID"
  fi
  if [[ -n "$EXPECTED_FIREFOX_ID" ]]; then
    echo "Expected Firefox ID: $EXPECTED_FIREFOX_ID"
  fi
  echo ""

  if ! command -v jq >/dev/null 2>&1; then
    log_fail "jq JSON parser is not available; cannot validate manifests"
    exit 1
  fi

  check_chromium_manifest
  check_firefox_manifest

  echo ""
  echo "Summary: ${pass_count} pass, ${warn_count} warn, ${fail_count} fail"

  if (( fail_count > 0 )); then
    exit 1
  fi
}

main "$@"
