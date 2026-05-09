# Architecture

## High-Level System Design

Website
↓
Browser Extension
↓
Native Messaging Host
↓
Rust Background Daemon
↓
PC/SC Layer
↓
USB Smartcard Reader
↓
German eID Card

---

## Desktop Application

The desktop app will:

- provide onboarding
- provide diagnostics
- manage settings
- show tray icon
- display notifications
- show card status
- show browser connection status

The desktop app communicates with the daemon over:
- local WebSocket
- local REST API
- Unix domain socket

---

## Daemon Responsibilities

The daemon is the core system component.

Responsibilities:

- smartcard detection
- PC/SC integration
- browser communication
- session management
- authentication lifecycle
- secure IPC
- logging
- diagnostics
- official stack integration

---

## Browser Extension Responsibilities

- detect supported websites
- initiate authentication
- communicate with local daemon
- launch login flows
- display browser-side UI

---

## Packaging Targets

- Snap
- Flatpak
- AppImage
- Debian package

---

## Linux Compatibility Goals

- Ubuntu 24+
- GNOME
- KDE Plasma
- Wayland
- X11
