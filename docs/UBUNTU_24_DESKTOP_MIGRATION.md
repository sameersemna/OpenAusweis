# Ubuntu 24.04 Desktop Migration

## Context

The desktop shell currently builds with Tauri v1, but Ubuntu 24.04 ships WebKitGTK/libsoup3 as the default stack.
Tauri v1 desktop runtime in this project expects a libsoup2-based stack, which leads to runtime incompatibilities when both libsoup2 and libsoup3 symbols are present.

This document tracks the migration path to a native Ubuntu 24.04-compatible desktop runtime.

## Goal

Run the desktop app on Ubuntu 24.04 without local linker hacks, local pkg-config shims, or mixed libsoup2/libsoup3 loading.

## Non-Goals

- Implementing new business features in desktop UI during migration.
- Changing daemon/native-host IPC contracts as part of desktop runtime migration.

## Current Status

- Temporary local compatibility shims were removed from repository state.
- Tauri CLI is available in the desktop workspace.
- Desktop config includes explicit tray configuration and icon path.
- Desktop app started migration to Tauri v2 config and dependency baselines.
- Frontend invoke path migrated to `@tauri-apps/api/core`.
- Tray configuration moved to Tauri v2 schema key `app.trayIcon`.
- Tauri v2 tray Rust wiring reintroduced using `tauri::tray::TrayIconBuilder` and v2 `menu` APIs.
- `cargo check -p openausweis-desktop` now succeeds on Tauri v2.
- `tauri dev` compiles and launches without libsoup symbol conflicts.
- In VS Code Snap shells, runtime may fail unless Snap-injected GTK/GIO variables are cleared.

## Migration Strategy

### Phase 1: Stabilize Current Desktop Runtime

1. Keep the current desktop codebase compiling and passing checks on supported environments.
2. Avoid committing local runtime hacks (`.pkgconfig`, `.libcompat`) to source control.
3. Preserve protocol and policy editor behavior in desktop commands/UI.

### Phase 2: Prepare Tauri v2 Upgrade Branch

1. Create a dedicated migration branch for desktop runtime upgrade.
2. Upgrade desktop JS dependencies to Tauri v2 packages.
3. Upgrade desktop Rust crates (`tauri`, `tauri-build`) to v2-compatible versions.
4. Replace v1 tray APIs with v2 tray APIs.
5. Rebuild and fix compiler/runtime API differences.

### Phase 3: Linux Validation on Ubuntu 24.04

1. Validate `npx tauri dev` without compatibility environment variables.
2. Validate tray behavior, window restore/quit actions, and command invocations.
3. Validate policy read/save + daemon probe from desktop UI.

### Phase 4: CI and Documentation

1. Add/adjust CI lane for desktop Linux checks in a suitable image.
2. Document runtime prerequisites for Ubuntu 24.04.
3. Remove obsolete migration notes once v2 path is stable.

## Technical Checklist

- [x] Tauri v2 dependency updates applied in desktop package and Rust manifest.
- [x] Tray setup migrated from v1 to v2 APIs.
- [x] Desktop commands compile and execute under v2 runtime.
- [x] Ubuntu 24.04 local `tauri dev` runs without libsoup symbol conflicts.
- [x] CI updated for desktop runtime coverage.

## Runtime Note (VS Code Snap)

If you run commands from VS Code installed as Snap, Snap environment variables can inject incompatible GTK/GIO runtime paths and trigger glibc symbol lookup failures.

Use:

- `npm run --workspace @openausweis/desktop tauri:dev:snap-safe`

## Risk Notes

- Tray API migration is the highest code-change risk area.
- Desktop build tooling changes may affect command names and config shape.
- This migration should be isolated from unrelated feature work to reduce regression risk.

## Migration Complete

All phases completed as of 2026-05-09.

- Tauri v2 runtime on Ubuntu 24.04 validated locally (daemon probe, policy save/load, tray Quit).
- CI lane added: `.github/workflows/ci.yml` `desktop` job runs `cargo fmt`, `cargo clippy`, `cargo check`, and `npm run typecheck` on `ubuntu-24.04` with WebKitGTK 4.1 system deps.
- VS Code Snap workaround documented and shipped as `tauri:dev:snap-safe` npm script.

This document is retained for historical context. No further migration work is required unless the Tauri major version changes again.
