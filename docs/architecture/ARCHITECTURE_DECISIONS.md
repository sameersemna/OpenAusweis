# Architecture Decisions

This document records major architectural decisions for OpenAusweis.

The purpose of this file is to:
- prevent uncontrolled architecture drift
- preserve long-term system consistency
- guide AI-assisted development
- document rationale behind key decisions
- reduce accidental refactors
- preserve Linux-first design principles

---

# ADR-001 — Linux-First Platform

Status: Accepted

Decision:
OpenAusweis is a Linux-first desktop platform focused primarily on Ubuntu.

Rationale:
- Linux desktop lacks polished citizen eID tooling
- Ubuntu has strong developer and enterprise adoption
- Linux system integration is a primary product differentiator

Implications:
- Linux UX takes priority over cross-platform abstractions
- Wayland compatibility is mandatory
- GNOME compatibility is mandatory
- KDE support is important but secondary

---

# ADR-002 — Rust Core Architecture

Status: Accepted

Decision:
The daemon, IPC layer, smartcard layer, and core middleware are implemented in Rust.

Rationale:
- strong memory safety
- excellent Linux integration
- async ecosystem maturity
- daemon/service suitability
- performance
- security

Implications:
- Tokio async runtime preferred
- modular crate structure required
- avoid unnecessary runtime overhead

---

# ADR-003 — Tauri Desktop Shell

Status: Accepted

Decision:
The desktop application uses Tauri.

Rationale:
- lightweight binaries
- native Linux integration
- tray support
- lower memory usage than Electron
- strong Rust interoperability

Implications:
- frontend separated from daemon
- desktop app acts primarily as UX shell
- daemon remains source of truth

---

# ADR-004 — Background Daemon Ownership

Status: Accepted

Decision:
Authentication state and smartcard lifecycle are owned by the daemon, not the UI.

Rationale:
- supports browser integration
- enables headless operation
- improves crash resilience
- separates UX from authentication state

Implications:
- UI must tolerate daemon restarts
- daemon remains long-running
- daemon may later become a system/user service

---

# ADR-005 — Browser Integration Model

Status: Accepted

Decision:
Browser integration uses:
Browser Extension → Native Messaging → Local Daemon

Rationale:
- industry-standard security model
- browser sandbox limitations
- Linux compatibility
- cross-browser feasibility

Implications:
- browser treated as untrusted
- daemon validates all requests
- extension remains thin

---

# ADR-006 — Official eID Stack Reuse

Status: Accepted

Decision:
OpenAusweis should integrate with official German eID components wherever feasible.

Rationale:
- avoids reimplementing sensitive cryptography
- reduces protocol risk
- improves compatibility
- accelerates development

Implications:
- do not manually implement PACE/EAC unless absolutely necessary
- prioritize interoperability over reinvention

---

# ADR-007 — IPC Architecture

Status: Accepted

Decision:
IPC uses local secure communication channels:
- WebSocket
- local REST endpoints
- Unix domain sockets where appropriate

Rationale:
- browser compatibility
- local daemon communication
- future SDK support

Implications:
- local-only binding required
- authentication boundaries required
- origin validation mandatory

---

# ADR-008 — Packaging Strategy

Status: Accepted

Decision:
Primary packaging targets:
- Snap
- Flatpak
- AppImage

Rationale:
- Ubuntu App Center compatibility
- Linux distribution reach
- easier onboarding

Implications:
- sandbox compatibility required
- PC/SC access constraints must be validated early
- packaging tests are mandatory

---

# ADR-009 — UX Philosophy

Status: Accepted

Decision:
The product should feel like:
- a polished Linux system utility
- a trustworthy authentication platform
- a lightweight background service

Rationale:
- security software must inspire confidence
- desktop integration is a differentiator
- frictionless UX increases adoption

Implications:
- tray-first UX
- minimal UI clutter
- fast startup
- dark mode mandatory

---

# ADR-010 — Security Model

Status: Accepted

Decision:
Security takes priority over feature velocity.

Rationale:
- authentication middleware is security-sensitive
- browser communication is inherently risky
- smartcard interactions require trust boundaries

Implications:
- validate all IPC
- avoid logging sensitive information
- least-privilege approach
- no telemetry by default

---

# ADR-011 — Architecture Stability

Status: Accepted

Decision:
Major architectural refactors require explicit review.

Rationale:
- AI-assisted coding can introduce architectural drift
- middleware systems degrade quickly under uncontrolled changes

Implications:
- incremental evolution preferred
- preserve modular boundaries
- preserve daemon ownership model

---

# Future ADRs

Future architectural changes must:
- include rationale
- include implications
- include migration considerations
- include rollback considerations