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
- Remaining blocker is framework/runtime compatibility with Ubuntu 24.04 library stack.

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

- [ ] Tauri v2 dependency updates applied in desktop package and Rust manifest.
- [ ] Tray setup migrated from v1 to v2 APIs.
- [ ] Desktop commands compile and execute under v2 runtime.
- [ ] Ubuntu 24.04 local `tauri dev` runs without libsoup symbol conflicts.
- [ ] CI updated for desktop runtime coverage.

## Risk Notes

- Tray API migration is the highest code-change risk area.
- Desktop build tooling changes may affect command names and config shape.
- This migration should be isolated from unrelated feature work to reduce regression risk.
