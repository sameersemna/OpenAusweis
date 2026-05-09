You are acting as a senior Linux platform architect and staff-level systems engineer.

Project:
OpenAusweis

Goal:
Build a Linux-native German eID desktop platform for Ubuntu using:
- Rust
- Tauri
- React
- TypeScript
- PC/SC smartcard integration
- Browser extension integration
- Native messaging
- Tray icon integration
- Wayland compatibility

IMPORTANT:
Do NOT implement German eID cryptography manually from scratch.
Do NOT reinvent official protocols unnecessarily.

The project should instead:
- create modern Linux infrastructure
- create a polished Ubuntu desktop experience
- integrate with existing official components where appropriate
- create browser integration architecture
- create modern developer tooling

Primary goals:
- Ubuntu App Center distribution
- top bar tray app
- browser login support
- smartcard reader support
- modular architecture
- modern UX

Your task:
FIRST analyze the project architecture deeply.

Then create:
1. detailed monorepo structure
2. recommended Rust crate structure
3. daemon architecture
4. IPC architecture
5. browser extension architecture
6. security model
7. packaging strategy
8. development workflow
9. GitHub Actions plan
10. local development strategy

DO NOT start coding immediately.

First produce:
- architecture analysis
- risks
- unknowns
- dependencies
- recommended milestones
- development phases

Then WAIT for approval before generating code.
