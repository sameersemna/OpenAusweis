# Phase 1 Plan

## Objective

Establish the foundational architecture for OpenAusweis.

## Deliverables

- Monorepo setup
- Rust workspace setup
- Tauri desktop shell
- Tray icon prototype
- IPC communication prototype
- Logging infrastructure
- GitHub Actions CI skeleton
- Initial documentation

## Constraints

- Linux-first
- Wayland compatible
- Minimal dependencies
- Security-first architecture
- No custom eID cryptography implementation

## Success Criteria

- Desktop app launches successfully
- Tray icon works on Ubuntu GNOME
- Rust daemon starts correctly
- IPC communication functions locally
- Project builds cleanly on Ubuntu 24+
