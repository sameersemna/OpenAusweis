# Desktop Compatibility Matrix

## Purpose

This document defines the validation surface for PHASE 2C: Linux Desktop Environment Validation and Hardening. The goal is to verify that OpenAusweis behaves predictably across the Linux desktop combinations that matter most to real users, without changing core architecture or adding new product features.

Primary focus areas:
- tray behavior
- notifications
- focus handling
- startup behavior
- browser handoff
- close-to-tray behavior
- PIN prompt attention behavior
- auth completion return flow
- packaging and runtime behavior
- accessibility behavior

## Scope

In scope:
- GNOME Wayland
- GNOME X11
- KDE Plasma Wayland
- KDE Plasma X11
- Firefox native
- Firefox Snap
- Chromium
- Chrome
- Brave
- dev mode
- AppImage
- Snap classic
- Snap strict
- Flatpak

Out of scope:
- major UI redesigns
- new auth features
- new desktop architecture
- broad cross-platform expansion beyond Linux desktop validation

## Current Validation Hooks

Existing scripts provide the baseline for this phase:
- [scripts/validate-linux-packaging.sh](../../scripts/validate-linux-packaging.sh)
- [scripts/validate-desktop-polish.sh](../../scripts/validate-desktop-polish.sh)
- `npm run validate:phase2c`

Useful related references:
- [docs/PHASE_2B_TESTING_STRATEGY.md](../PHASE_2B_TESTING_STRATEGY.md)
- [docs/architecture/ARCHITECTURE_DECISIONS.md](../architecture/ARCHITECTURE_DECISIONS.md)
- [snap/snapcraft.yaml](../../snap/snapcraft.yaml)

## Desktop Environment Matrix

| Desktop | Session | Priority | Expected behavior | Main risks |
|---|---|---:|---|---|
| GNOME | Wayland | High | Tray fallback is stable, notifications are restrained, focus changes are minimal, and browser handoff is obvious | Tray visibility may be inconsistent; focus-stealing can feel worse on Wayland; shell extensions may affect behavior |
| GNOME | X11 | High | Tray and focus behavior remain predictable, and auth completion returns cleanly to the browser | X11 can hide Wayland-only issues; tray behavior may appear better than the target baseline |
| KDE Plasma | Wayland | High | Tray is visible when supported, notifications are usable, and close-to-tray works without repeated prompts | Plasma tray integrations vary by distro and panel configuration; focus and activation can differ from GNOME |
| KDE Plasma | X11 | Medium | Core auth flow remains stable and browser handoff is consistent | X11 can mask compositor-specific issues; panel/tray setup may influence visibility and attention behavior |

### Desktop-specific validation expectations

1. The desktop app should start without surfacing technical warnings in the primary UI.
2. The tray should remain the main recovery path when the window is closed.
3. PIN-required transitions should request attention once, not repeatedly.
4. Completion should hand control back to the browser with a calm success cue.
5. Tray limitations should degrade into notifications, not into noisy error loops.

## Browser and Runtime Matrix

| Browser/runtime | Priority | Expected behavior | Main risks |
|---|---|---|---|
| Firefox native | High | Native messaging host loads reliably and receives allowed origins correctly | System profile differences can affect manifest installation and permissions |
| Firefox Snap | High | Native messaging works through Snap confinement and the host path remains reachable | Snap confinement can block host access or isolate expected paths |
| Chromium | High | Manifest installs correctly and browser handoff returns to the desktop app | Extension ID drift or manifest placement problems can break handoff |
| Chrome | High | Same behavior as Chromium with consistent native messaging | Vendor-specific profile paths can differ from Chromium defaults |
| Brave | Medium | Same native messaging behavior as Chromium-family browsers | Browser-specific profile layout and manifest path differences can appear |

### Browser-specific validation expectations

1. The browser extension should be able to start a session from an allowed origin.
2. The native host should forward the request to the daemon without manual intervention.
3. The desktop should reflect session progress and completion consistently.
4. Failure states should remain plain-language and avoid backend jargon.

## Packaging Matrix

| Packaging mode | Priority | Expected behavior | Main risks |
|---|---|---|---|
| Dev mode | High | Fast iteration, clear logs, and local daemon/browser pairing | Environment pollution from shell variables or stale build outputs |
| AppImage | High | Portable runtime behavior with predictable desktop integration | Runtime library assumptions, desktop file registration, and socket accessibility |
| Snap classic | High | Desktop integration should remain close to native behavior | Confined filesystem access and host integration quirks |
| Snap strict | High | Confinement must not block the daemon, native host, or browser handoff | Interface and path restrictions are the dominant risk |
| Flatpak | Medium | Runtime should remain functional under sandboxed desktop constraints | Portal behavior, filesystem access, and browser/native-host reachability |

### Packaging expectations

1. The desktop binary should locate the daemon socket in a predictable runtime directory.
2. Native messaging manifests should be installable and discoverable in the expected per-browser locations.
3. The packaging mode should not change core auth flow semantics.
4. If tray support is limited, the app should degrade gracefully to notifications and window fallback.

## Environment-Specific Risks

### Wayland-specific concerns

- tray availability depends on the compositor and desktop shell integration
- focus behavior is more sensitive to activation timing and window raising
- repeated attention requests can feel disruptive if the window is hidden
- native desktop assumptions based on X11 can produce false positives

### GNOME-specific concerns

- GNOME Wayland is the most important compatibility baseline
- AppIndicator-style tray behavior may require fallback handling
- shell extensions may alter notification and tray visibility behavior
- the app should avoid fighting GNOME's focus model

### KDE-specific concerns

- Plasma tray behavior is often better than GNOME's, but not uniform across panel layouts
- Wayland focus and activation can vary by compositor version
- notification presentation may differ from GNOME in subtle ways
- panel/tray configuration should not be assumed to match the default install

### Packaging concerns

- Snap confinement can hide filesystem or socket assumptions that work in dev mode
- Flatpak sandboxing can expose missing portal or permission assumptions
- AppImage may work locally but fail to integrate with host desktop services cleanly
- packaging should not encode brittle paths outside the runtime/socket conventions already established in the repo

### Browser/runtime concerns

- Firefox Snap is the most likely place for host discovery or manifest path issues
- Chromium-family browsers can drift in extension ID or host manifest placement
- browser profile differences can hide install problems if only one profile is exercised
- native messaging failures should be surfaced as clear operational errors, not as raw protocol text

## Validation Strategy

### 1. Smoke validation

Use this first when testing a new machine, image, or packaging artifact:
- confirm Linux baseline and desktop/session type
- verify daemon startup and socket creation
- confirm native messaging manifests are installed
- confirm the browser can reach the native host
- confirm the desktop enters and exits auth flow cleanly

### 2. Manual QA sequence

Run the desktop polish checklist from [scripts/validate-desktop-polish.sh](../../scripts/validate-desktop-polish.sh) in each target desktop/session combination:
1. Launch with no active session and confirm the home view stays calm.
2. Stop or disconnect the daemon briefly and confirm the UI shows recovery, not backend wording.
3. Start a browser sign-in and confirm the desktop moves into PIN-required or in-progress state.
4. Hide or close the window before PIN entry and confirm it is raised once when attention is needed.
5. Close the window and confirm the app remains available via tray or notification fallback.
6. Submit an invalid PIN and confirm remaining-attempt guidance is plain-language.
7. Exhaust invalid attempts and confirm the user is told to start again.
8. Complete the flow and confirm the browser regains control cleanly.

### 3. Packaging/runtime validation

Run [scripts/validate-linux-packaging.sh](../../scripts/validate-linux-packaging.sh) on each packaging target and browser combination. The script should remain the first-line automated check for:
- Linux baseline verification
- desktop/session detection
- daemon socket sanity
- native host manifest discovery
- browser manifest ID drift
- Snap/Flatpak availability signals
- pcscd runtime availability

### 4. Accessibility validation

Manual accessibility checks remain important because desktop integration issues often appear as attention, focus, or discoverability failures rather than pure functional bugs.

Validate that:
- the PIN prompt is obvious when attention is requested
- the return-to-browser cue is understandable without relying on color alone
- close-to-tray behavior is discoverable when the window closes
- notifications remain short and actionable

## Recommended Compatibility Matrix

| Item | Dev mode | AppImage | Snap classic | Snap strict | Flatpak |
|---|---|---|---|---|---|
| GNOME Wayland | Required | Required | Required | Required | Required |
| GNOME X11 | Required | Recommended | Required | Required | Recommended |
| KDE Plasma Wayland | Recommended | Recommended | Required | Required | Recommended |
| KDE Plasma X11 | Recommended | Recommended | Recommended | Recommended | Optional |
| Firefox native | Required | Required | Required | Required | Required |
| Firefox Snap | Recommended | Recommended | Required | Required | Recommended |
| Chromium | Required | Required | Required | Required | Required |
| Chrome | Required | Required | Required | Required | Required |
| Brave | Recommended | Recommended | Recommended | Recommended | Recommended |

Legend:
- Required: must be exercised before release sign-off for this phase
- Recommended: should be covered when available in CI, lab, or contributor machines
- Optional: useful for broader confidence, but not a release gate

## Automated Validation Opportunities

These are good candidates for incremental automation without changing architecture:
- add environment detection and reporting to the compatibility matrix generator or validation scripts
- assert that tray and notification fallback flags are emitted correctly for GNOME Wayland
- add browser profile path checks for Firefox Snap and Chromium-family profiles
- validate that packaging manifests point to executable host binaries and installed paths
- make session-type and desktop-type warnings actionable in the validation output
- add regression checks for close-to-tray and one-shot attention behavior through existing desktop instrumentation

## Rollback Strategy

If a desktop- or packaging-specific change regresses behavior:
1. revert the packaging tweak or desktop integration change first, not the auth core
2. keep the daemon, IPC, and browser protocol stable while isolating the desktop regression
3. disable only the affected packaging target or shell-specific behavior while the issue is investigated
4. fall back to the last known-good packaging artifact for release candidates
5. preserve validation output and logs so the regression can be reproduced on the same desktop/session/browser combination

Rollback priority order:
- browser/runtime integration changes
- packaging and manifest changes
- tray and notification behavior changes
- any desktop attention or focus adjustment

## Exit Criteria For PHASE 2C

PHASE 2C is ready to close when:
- the matrix above has been exercised on GNOME Wayland, GNOME X11, KDE Wayland, and KDE X11 where available
- Firefox native, Firefox Snap, Chromium, Chrome, and Brave have been validated at least once in a supported desktop session
- dev mode, AppImage, Snap classic, Snap strict, and Flatpak have been verified for runtime and packaging sanity
- tray, notifications, focus, browser handoff, close-to-tray, and auth completion behave consistently enough to avoid environment-specific user instructions
- any remaining edge cases are documented with a workaround or a rollback plan
