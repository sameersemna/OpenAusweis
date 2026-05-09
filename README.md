# OpenAusweis

OpenAusweis is a modern Linux-native German eID desktop platform focused on Ubuntu and Linux desktop integration.

The project aims to provide:

- Native Linux desktop UX comparable to the official macOS AusweisApp
- Browser authentication support using German eID cards
- Tray/top-bar integration
- PC/SC smartcard reader support
- Browser extension integration
- Ubuntu App Center distribution
- Modern Linux-first architecture
- Open-source extensibility

---

# Goals

- Linux-native experience
- Security-first architecture
- Modern UX
- Wayland compatibility
- Browser login support
- Developer-friendly architecture

---

# Planned Stack

| Layer | Technology |
|---|---|
| Core daemon | Rust |
| Desktop app | Tauri |
| Frontend | React + TypeScript |
| Browser extension | TypeScript |
| Smartcard | pcsc-lite |
| Packaging | Snap / Flatpak / AppImage |

---

# Repository Structure

```text
apps/
  daemon/                # Rust daemon with Unix socket IPC endpoint
  ausweisapp2-bridge/    # Rust stdin/stdout bridge to local AusweisApp2 WebSocket
  native-host/           # Browser native messaging host bridge
  desktop/               # React frontend + Tauri shell
    src-tauri/           # Native desktop runtime and tray integration
  browser-extension/     # Chromium/Firefox extension scaffold

crates/
  openausweis-ipc/       # Shared Rust IPC contracts
  openausweis-core/      # Core service traits and domain interfaces
  openausweis-pcsc/      # PC/SC integration boundary (placeholder)

packages/
  shared-types/          # Shared TypeScript message contracts

docs/
  DEVELOPMENT_WORKFLOW.md
  ...existing architecture/planning docs
```

---

# Current Status

Foundation implementation started.

Ubuntu 24.04 desktop migration started:

- Track progress in `docs/UBUNTU_24_DESKTOP_MIGRATION.md`
- Current desktop runtime migration focus is Tauri upgrade for native libsoup3-compatible execution on Ubuntu 24.04

Implemented in this baseline:

- Cargo workspace and Rust crate boundaries
- Daemon process scaffold with Unix domain socket endpoint
- Typed IPC contracts in Rust and TypeScript
- Tauri desktop shell with tray menu baseline
- React desktop UI shell for diagnostics/status
- Browser extension scaffold with native messaging bridge
- GitHub Actions CI workflows
- Local development scripts and workflow doc

Implemented for Phase 2 (in progress):

- PC/SC context probing in daemon using `pcsc-lite`
- Reader enumeration and card presence snapshot in daemon status responses
- Desktop UI reader/card status panel with daemon watch stream updates
- Daemon watch stream suppresses unchanged status frames to reduce IPC noise
- Diagnostic messages surfaced in UI for reader/card detection failures

Desktop run note (VS Code Snap on Linux):

- Use `npm run --workspace @openausweis/desktop tauri:dev:snap-safe` to avoid Snap-injected GTK/GIO runtime path conflicts.

Auth executor mode (session PIN submit path):

- `OPENAUSWEIS_AUTH_EXECUTOR=mock` (default): uses the internal placeholder flow and completes sessions locally.
- `OPENAUSWEIS_AUTH_EXECUTOR=ausweisapp2`: verifies the `ausweisapp2` binary and delegates authentication through a bridge process.

AusweisApp2 bridge environment:

- `OPENAUSWEIS_AUSWEISAPP2_BRIDGE_BIN` (required in `ausweisapp2` mode): executable used by daemon for delegated auth.
- `OPENAUSWEIS_AUSWEISAPP2_BRIDGE_ARGS` (optional): whitespace-separated arguments passed to bridge executable.
- `OPENAUSWEIS_AUSWEISAPP2_BRIDGE_TIMEOUT_MS` (optional): bridge timeout in milliseconds (default: `20000`).
- `OPENAUSWEIS_AA2_WS_URL` (optional, bridge env): AusweisApp2 WebSocket URL (default: `ws://127.0.0.1:24727`).
- `OPENAUSWEIS_AA2_WS_AUTH_REQUEST` (optional, bridge env): JSON text sent to AusweisApp2; supports `{session_id}` placeholder.
- `OPENAUSWEIS_AA2_WS_TIMEOUT_MS` (optional, bridge env): bridge WebSocket operation timeout in milliseconds (default: `15000`).
- `OPENAUSWEIS_AA2_STRICT_SUCCESS` (optional, bridge env): set to `true`/`1`/`yes`/`on` to require an explicit success marker.
- `OPENAUSWEIS_AA2_SUCCESS_MAJORS` (optional, bridge env): comma-separated allowlist used in strict mode (default: `ACCESS_RIGHTS,ACCEPTED,AUTH,AUTHENTICATED`).
- `OPENAUSWEIS_AA2_DIAGNOSTICS` (optional, bridge env): set to `true`/`1`/`yes`/`on` to print startup diagnostics (ws url, timeout, strict mode, allowlist, request size) to stderr.

Bridge contract (daemon -> bridge over stdin/stdout):

- Request JSON line:
  - `{"protocol_version":1,"action":"authenticate","session_id":"<uuid>"}`
- Response JSON line:
  - Success: `{"ok":true}`
  - Failure: `{"ok":false,"error":"<message>"}`

AusweisApp2 WebSocket response evaluation in bridge:

- Treated as failure when response contains protocol error markers such as:
  - top-level `major: "error"`
  - top-level `ok: false`
  - nested `result.major: "error"`
  - non-empty `error` field (top-level or nested in `result`)
- In default mode, non-error responses are treated as successful and bridge returns `{"ok":true}`.
- In strict mode (`OPENAUSWEIS_AA2_STRICT_SUCCESS=true`), success also requires an explicit marker:
  - top-level `ok: true`, or
  - top-level `major`/`msg` in allowlist, or
  - nested `result.major` in allowlist.

Example:

```bash
OPENAUSWEIS_AUTH_EXECUTOR=mock npm run --workspace @openausweis/desktop tauri:dev:snap-safe
```

One-command local tuning flow (build bridge + strict mode + diagnostics):

```bash
npm run dev:aa2
```

This command runs `scripts/run-desktop-ausweisapp2.sh`, which:

- builds `openausweis-ausweisapp2-bridge`
- sets `OPENAUSWEIS_AUTH_EXECUTOR=ausweisapp2`
- sets default `OPENAUSWEIS_AA2_STRICT_SUCCESS=true`
- sets default `OPENAUSWEIS_AA2_DIAGNOSTICS=true`
- uses `OPENAUSWEIS_AUSWEISAPP2_BRIDGE_BIN=./target/debug/openausweis-ausweisapp2-bridge` unless already set
- launches `tauri:dev:snap-safe` when running under VS Code Snap (otherwise `tauri:dev`)

AusweisApp2 mode example:

```bash
cargo build -p openausweis-ausweisapp2-bridge

OPENAUSWEIS_AUTH_EXECUTOR=ausweisapp2 \
OPENAUSWEIS_AUSWEISAPP2_BRIDGE_BIN=./target/debug/openausweis-ausweisapp2-bridge \
npm run --workspace @openausweis/desktop tauri:dev:snap-safe
```

Smartcard runtime requirements (Ubuntu 24.04+):

```bash
sudo apt install -y pcscd libpcsclite1 libpcsclite-dev pkgconf pcsc-tools
sudo systemctl enable --now pcscd
```

---

# License

MIT