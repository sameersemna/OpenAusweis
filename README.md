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
  desktop/
  daemon/
  browser-extension/

packages/
  ipc/
  sdk/
  shared-types/

docs/
  architecture/
  planning/
  security/
  ux/
  vision/
  copilot/
```

---

# Current Status

Early architecture and planning phase.

---

# License

MIT