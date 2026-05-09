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

Implemented in this baseline:

- Cargo workspace and Rust crate boundaries
- Daemon process scaffold with Unix domain socket endpoint
- Typed IPC contracts in Rust and TypeScript
- Tauri desktop shell with tray menu baseline
- React desktop UI shell for diagnostics/status
- Browser extension scaffold with native messaging bridge
- GitHub Actions CI workflows
- Local development scripts and workflow doc

---

# License

MIT