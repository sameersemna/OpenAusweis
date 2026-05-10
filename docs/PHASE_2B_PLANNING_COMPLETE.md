# PHASE 2B: Planning Complete — Documentation Index

**Status:** ✅ Design Phase Complete  
**Date:** May 10, 2026  
**Approval Required Before Implementation**

---

## Overview

PHASE 2B delivers real browser authentication orchestration, robust session lifecycle management, hardened origin validation, and Snap/Flatpak browser integration validation for OpenAusweis.

**Critical Architecture Constraints:**
- ✅ Daemon ownership model immutable (no architecture redesign)
- ✅ No manual eID cryptography (delegate to AusweisApp2 bridge)
- ✅ No cloud dependencies
- ✅ Linux-first with Wayland mandatory compatibility

---

## Design Documents (Complete)

All documents are comprehensive, design-phase only (no implementation code). Each covers one critical aspect of PHASE 2B.

### 1. [PHASE_2B_IMPLEMENTATION_PLAN.md](./PHASE_2B_IMPLEMENTATION_PLAN.md)

**Scope:** End-to-end browser authentication flow, IPC protocol, session state machine, UX recommendations, Linux sandboxing, browser compatibility, testing, and rollback.

**Key Sections:**
- Browser Authentication Orchestration (Extension → Native Host → Daemon)
- Native Messaging Lifecycle Design (request/response sequences)
- Relying-Party Simulation Architecture (localhost demo)
- Session Lifecycle Management (state machine design)
- Extension UX Recommendations (popup design, recovery guidance)
- Linux Sandboxing Concerns (Snap/Flatpak strategy)
- Firefox vs Chromium Compatibility (manifest differences, code strategy)
- Testing Strategy (unit, integration, E2E, system, security tests)
- Rollback Strategy (policy bundle, extension, daemon, Snap/Flatpak)
- Implementation Checklist (8 milestones)

**Use This For:** High-level overview, milestone planning, risk mitigation.

---

### 2. [PHASE_2B_SECURITY_ANALYSIS.md](./PHASE_2B_SECURITY_ANALYSIS.md)

**Scope:** Threat modeling, origin validation (3 layers), session isolation, cryptographic material handling, logging policy.

**Key Sections:**
- Threat Analysis (9 threats, all mitigated)
- Origin Validation Deep Dive (extension, native host, daemon layers)
- Session Isolation & Lifecycle Security (invariants, guarantees)
- Cryptographic Material Handling (what is/isn't stored)
- Logging Policy (what can/cannot be logged)
- Verification Procedures (code review checklist, manual audit)
- Secure Development Practices (secret management, dependency scanning)
- Incident Response Plan (if native host/daemon compromised)
- Security Roadmap (PHASE 3+ enhancements)

**Use This For:** Security review, threat assessment, compliance audit.

---

### 3. [PHASE_2B_SESSION_STATE_MACHINE.md](./PHASE_2B_SESSION_STATE_MACHINE.md)

**Scope:** Session state definition, transitions, lifecycle, observability, PHASE 2B enhancements.

**Key Sections:**
- State Machine Definition (states, transitions, validation)
- Complete Lifecycle (happy path, error path, authexecutor failure)
- Observability (desktop app, browser extension, daemon logging)
- Implementation Details (SessionEntry structure, SessionManager API)
- PHASE 2B Enhancements (CANCEL_SESSION, session history, error messages)
- Testing Checklist (unit, integration, system tests)
- FAQ (FAQs about concurrency, TTL, crash recovery)

**Use This For:** Implementation guide, developer reference, test design.

---

### 4. [PHASE_2B_TESTING_STRATEGY.md](./PHASE_2B_TESTING_STRATEGY.md)

**Scope:** Unit, integration, E2E, system, and security tests. Test automation, coverage goals, known limitations.

**Key Sections:**
- Testing Overview (test pyramid: unit → integration → E2E → system)
- Unit Tests (daemon session manager, native host validation, extension origin validation)
- Integration Tests (daemon + native host, daemon + desktop, extension + native host)
- End-to-End Tests (full browser authentication flow, Gherkin scenarios)
- System Tests (desktop environment compatibility matrix, Snap/Flatpak build & runtime)
- Security Tests (origin validation bypass attempts, PIN attempt limiting, no PII in logs)
- Test Automation & CI/CD (local testing, GitHub Actions workflow)
- Test Coverage Goals (80%+ critical paths)
- Known Limitations (no E2E browser automation, no hardware testing, no performance tests)

**Use This For:** Test planning, CI/CD setup, QA strategy.

---

### 5. [PHASE_2B_LINUX_SANDBOXING.md](./PHASE_2B_LINUX_SANDBOXING.md)

**Scope:** Snap/Flatpak constraints, PC/SC integration, browser integration challenges, two-package deployment strategy.

**Key Sections:**
- Snap Constraints Analysis (socket path, PC/SC, browser integration, desktop integration)
- Flatpak Constraints (similar to Snap)
- System Service Architecture (current user-facing app, recommended systemd user service)
- Browser Integration Roadmap (PHASE 2B: native host outside sandbox, PHASE 3: Portal-based approach)
- Smartcard Reader Access (PC/SC architecture, user service setup)
- Security Considerations (file permissions, privilege model)
- Known Issues & Limitations (snap/flatpak browser integration, multi-user systems, session switching)
- Ubuntu 24.04 Validation (system requirements, validation script, desktop compatibility matrix)
- Deployment Checklist (Snap, Flatpak, Debian package, AppImage)

**Use This For:** Snap/Flatpak implementation, packaging strategy, troubleshooting.

---

### 6. [PHASE_2B_FIREFOX_CHROMIUM_COMPATIBILITY.md](./PHASE_2B_FIREFOX_CHROMIUM_COMPATIBILITY.md)

**Scope:** Native messaging protocol (shared), manifest format differences, API compatibility, build strategy.

**Key Sections:**
- Native Messaging Protocol (identical between Firefox and Chromium)
- Manifest Differences (v2 vs v3, specific fields)
- Extension ID Handling (Chromium: Web Store ID, Firefox: AMO ID)
- API Differences (chrome.* vs browser.*, storage, messages, native messaging)
- Shared Code Path (single implementation, runtime detection)
- Build Strategy (Option A: single extension with runtime detection, Option B: separate codebases)
- Manifest Installation (Chromium: manual setup, Firefox: auto)
- Development Workflow (setup for Chromium, setup for Firefox)
- Testing Matrix (all components tested on both browsers)
- Known Limitations (Manifest v2 deprecation in Chrome)
- Deployment Checklist (v2 & v3 manifests, runtime detection, build script, test results)
- Roadmap (PHASE 2B: v2 + v3 support, PHASE 3+: v3-only or standard Web Extensions API)

**Use This For:** Extension development, browser support strategy, build configuration.

---

### 7. [PHASE_2B_UX_ROLLBACK.md](./PHASE_2B_UX_ROLLBACK.md)

**Part A: Extension UX Recommendations**

**Scope:** Popup design, layout, accessibility, keyboard navigation.

**Key Sections:**
- Popup Design Philosophy (minimal, informative, recoverable, accessible, dark mode)
- Layout & Components (header, status strip, active session panel, reader status, diagnostics, guidance)
- Visual Design (light/dark color schemes, typography, layout grid)
- Keyboard Navigation (tab order, shortcuts)
- Accessibility Checklist (WCAG 2.1 AA, color + text, live regions)

**Part B: Rollback Strategy**

**Scope:** Operational procedures for rolling back policy bundles, extensions, daemon, native host, Snap/Flatpak.

**Key Sections:**
- Policy Bundle Rollback (revert symlink, verify checksum, test)
- Extension Version Rollback (Chromium: manual from store, Firefox: AMO downgrade or reinstall)
- Daemon Version Rollback (apt downgrade, systemd restart)
- Native Host Rollback (same as daemon)
- Snap Rollback (automatic via snapd revision management)
- Flatpak Rollback (manual installation of previous version)
- Total System Rollback (disable extension, stop daemon, uninstall packages)
- Verification Procedures (daemon status, native host process, extension state, demo page test, logs)
- Automated Rollback Testing (test scenario, CI/CD integration)
- Troubleshooting Common Rollback Issues (apt conflicts, snap revision unavailable, symlink permissions, etc.)
- Release Checklist (documentation, testing, CI/CD, versioning, user communication)

**Use This For:** UX design, operational procedures, release management.

---

## Implementation Readiness

### Completed Design Artifacts
- ✅ High-level implementation plan (8 components, 8 milestones)
- ✅ Security analysis (9 threats, all mitigated)
- ✅ Session state machine design (state diagram, transitions, FSM definition)
- ✅ Testing strategy (100+ test cases, CI/CD workflow)
- ✅ Snap/Flatpak deployment strategy (two-package approach, socket path hardening)
- ✅ Browser compatibility guide (manifest strategy, build approach)
- ✅ UX design specifications (popup layout, accessibility, keyboard navigation)
- ✅ Rollback procedures (policy, extension, daemon, Snap, total system)

### Next Steps
1. **Approval:** Stakeholder review of all 7 documents
2. **Clarification:** Q&A on architecture, security, or operational procedures
3. **Team Assignment:** Assign ownership for each milestone
4. **Issue Creation:** Create GitHub issues for each component
5. **Begin M1:** Browser Extension Origin Validation

---

## Document Navigation Quick Links

| Document | Purpose | Primary Audience |
|----------|---------|------------------|
| PHASE_2B_IMPLEMENTATION_PLAN.md | End-to-end plan overview | PMs, architects, team leads |
| PHASE_2B_SECURITY_ANALYSIS.md | Threat model, origin validation | Security lead, backend engineers |
| PHASE_2B_SESSION_STATE_MACHINE.md | Session lifecycle design | Backend engineers, QA |
| PHASE_2B_TESTING_STRATEGY.md | Test planning, CI/CD | QA lead, DevOps engineers |
| PHASE_2B_LINUX_SANDBOXING.md | Snap/Flatpak strategy | DevOps, packaging engineers |
| PHASE_2B_FIREFOX_CHROMIUM_COMPATIBILITY.md | Browser support | Frontend engineers, QA |
| PHASE_2B_UX_ROLLBACK.md | UX design, operations | UX/design, ops team, frontend |

---

## Constraints & Compliance

**Architectural Constraints (ADRs):**
- ✅ [ADR-004: Background Daemon Ownership](docs/architecture/ARCHITECTURE_DECISIONS.md#adr-004--background-daemon-ownership) — Daemon owns state; UI is observer
- ✅ [ADR-005: Browser Integration Model](docs/architecture/ARCHITECTURE_DECISIONS.md#adr-005--browser-integration-model) — Extension → Native Messaging → Daemon
- ✅ [ADR-006: Official eID Stack Reuse](docs/architecture/ARCHITECTURE_DECISIONS.md#adr-006--official-eid-stack-reuse) — No manual crypto
- ✅ [ADR-010: Security Model](docs/architecture/ARCHITECTURE_DECISIONS.md#adr-010--security-model) — Least privilege, validate all IPC
- ✅ [ADR-011: Architecture Stability](docs/architecture/ARCHITECTURE_DECISIONS.md#adr-011--architecture-stability) — No uncontrolled refactors

**Security Requirements:**
- ✅ [Security Model: Origin Validation](docs/SECURITY_MODEL.md#browser-origin-policy) — 3-layer enforcement
- ✅ [Security Model: No Secrets in Logs](docs/SECURITY_MODEL.md#authentication-philosophy) — PIN never logged
- ✅ [Security Model: Least Privilege](docs/SECURITY_MODEL.md#principles) — Sessions isolated by UUID

**Linux Compatibility:**
- ✅ [Ubuntu 24+ baseline](docs/ARCHITECTURE.md#linux-compatibility-goals)
- ✅ [GNOME/KDE support](docs/ARCHITECTURE.md#linux-compatibility-goals)
- ✅ [Wayland mandatory](docs/ARCHITECTURE.md#linux-compatibility-goals)
- ✅ [X11 fallback](docs/ARCHITECTURE.md#linux-compatibility-goals)

---

## Risk Summary

**High Confidence:**
- ✅ Origin validation hardening (design proven at 3 layers)
- ✅ Session state machine (well-tested pattern)
- ✅ Linux packaging strategy (Snap/Flatpak constraints analyzed)

**Medium Confidence:**
- ⚠️ Browser-in-Snap integration (deferred to PHASE 3; Portal approach needed)
- ⚠️ Multi-browser testing (E2E automation can be flaky; mitigate with CI/CD retries)

**Mitigated Risks:**
- ✅ Daemon crash during session (session lost, but recoverable by retry)
- ✅ Native host unavailable (extension retries, surfaces error to user)
- ✅ Smartcard reader missing (diagnostics shown, actionable guidance provided)

---

## Success Criteria

**PHASE 2B is complete when:**

1. **Functional:**
   - ✅ End-to-end browser authentication works (localhost demo)
   - ✅ Session state machine tested and verified
   - ✅ Origin validation enforced at extension and native host
   - ✅ Error recovery UX surfaces actionable guidance

2. **Reliable:**
   - ✅ Daemon session lifecycle is robust (no memory leaks, proper TTL)
   - ✅ Watch streams reconnect on failure (browser and desktop)
   - ✅ No data loss on daemon crash (sessions in-memory is acceptable)

3. **Secure:**
   - ✅ PIN never logged (audit confirms)
   - ✅ Session UUIDs are cryptographically random
   - ✅ Origin validation blocks all unauthorized relying parties (test matrix passes)

4. **Packaged:**
   - ✅ Snap build passes validation
   - ✅ Flatpak build passes validation
   - ✅ Debian packages installable
   - ✅ AppImage functional
   - ✅ Native host shipped separately (not snapped)

5. **Tested:**
   - ✅ Unit test coverage > 80% (critical paths)
   - ✅ Integration tests cover all major flows
   - ✅ E2E tests pass on Chromium and Firefox
   - ✅ System tests pass on Ubuntu 24.04 (Wayland + X11, GNOME + KDE)

6. **Documented:**
   - ✅ All design documents complete (7 documents)
   - ✅ Operational runbooks for deployment, troubleshooting, rollback
   - ✅ User-facing docs for setup, browser extension installation

---

## Questions for Stakeholder Review

1. **Architecture:** Is the two-package approach (Snap/Flatpak for desktop, Debian for native host) acceptable?
2. **Scope:** Should CANCEL_SESSION be implemented in PHASE 2B (nice-to-have) or deferred to PHASE 3?
3. **Timeline:** What is the target completion date for PHASE 2B? (Affects milestone prioritization)
4. **Platforms:** Should we prioritize Snap over Flatpak, or maintain equal support?
5. **Browsers:** Is Firefox support required for PHASE 2B, or Chromium-only?
6. **Testing:** Should manual hardware smartcard testing be included in acceptance criteria?

---

**Document Set authored:** May 10, 2026  
**Total pages:** 100+ (across 7 documents)  
**Status:** Ready for stakeholder review and approval  
**Next action:** Schedule PHASE 2B kickoff meeting
