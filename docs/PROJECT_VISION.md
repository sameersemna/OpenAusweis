# OpenAusweis

## Vision

OpenAusweis is a modern Linux-native German eID desktop platform for Ubuntu and other Linux distributions.

The project aims to provide:

- A polished desktop experience comparable to the official macOS AusweisApp
- Seamless browser authentication using German eID cards
- Native Linux tray integration
- Smartcard reader support using PC/SC
- Browser extension integration
- Secure local authentication middleware
- Ubuntu App Center distribution
- Flatpak and AppImage support
- Modern UX and onboarding
- Developer-friendly architecture
- Open-source extensibility

The project is NOT intended to replace the official eID cryptographic stack initially.

Instead, it will:
- integrate with existing official components where possible
- modernize the Linux experience
- improve browser integration
- improve developer tooling
- provide a modern architecture for future expansion

## Primary Target

Ubuntu Linux desktop users.

## Secondary Targets

- Fedora
- Debian
- Arch Linux
- NixOS
- KDE Plasma
- GNOME

## Main Components

- Rust daemon/service
- Tauri desktop application
- Browser extension
- Smartcard integration layer
- Local IPC/API bridge
- Packaging/distribution layer

## Core Principles

- Linux-first
- Security-first
- Privacy-first
- Modern UX
- Minimal resource usage
- Open-source
- Modular architecture
- Wayland compatibility
