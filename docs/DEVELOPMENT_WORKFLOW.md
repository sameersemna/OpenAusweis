# Development Workflow

## Prerequisites

- Rust stable toolchain
- Node.js 22+
- npm 10+
- pcsc-lite runtime and headers for future smartcard integration
- For Tauri desktop compilation on Ubuntu 24.04+:

```bash
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  pkg-config
```

> **VS Code Snap users:** run the desktop via `npm run --workspace @openausweis/desktop tauri:dev:snap-safe`
> to strip Snap-injected GTK/GIO environment variables that cause runtime symbol errors.

## Install

```bash
npm install
cargo fetch
```

## Daily Commands

```bash
# Rust checks
./scripts/check-rust.sh

# Optional desktop (Tauri) check once system deps are installed
cargo check -p openausweis-desktop

# Start daemon and desktop UI together
./scripts/dev-up.sh

# Run daemon only
./scripts/run-daemon.sh

# Run native messaging host bridge
./scripts/run-native-host.sh

# Optional: override native-host relying-party allowlist for local testing
OPENAUSWEIS_ALLOWED_EXACT_ORIGINS="http://localhost,https://localhost" \
OPENAUSWEIS_ALLOWED_SUFFIXES=".bundid.de,.bund.de" \
./scripts/run-native-host.sh
```

## Browser Extension Local Setup

1. Load [apps/browser-extension](../apps/browser-extension) as unpacked extension.
2. Wire native host manifest from [apps/browser-extension/src/native-messaging-host.json](../apps/browser-extension/src/native-messaging-host.json).
3. Replace the extension ID placeholder in manifest before testing native messaging.

## Notes

- Native messaging host binary path is currently a placeholder.
- Browser extension and native host now use a versioned IPC envelope (`protocol_version`, `request_id`, `payload`).
- Native host uses the browser-native length-prefixed JSON framing on stdio.
- Desktop app can read/write the policy bundle at `~/.config/openausweis/origin-policy/current/`.
- The bundle contains `policy.json` plus `policy.sha256` as the integrity sidecar.
- Policy writes publish a new versioned bundle directory and then atomically swap the `current` symlink.
- Override policy bundle root with `OPENAUSWEIS_POLICY_DIR` for desktop and native-host.
- `OPENAUSWEIS_POLICY_FILE` is still accepted as a legacy compatibility path for reads.
- eID cryptographic operations are intentionally delegated to official components in future milestones.
