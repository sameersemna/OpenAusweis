# PHASE 2B: Linux Sandboxing & Packaging Concerns

**Status:** Design Phase  
**Date:** May 10, 2026  
**Scope:** Snap/Flatpak constraints, browser integration, system service architecture

---

## Executive Summary

OpenAusweis targets multiple packaging formats (Snap, Flatpak, AppImage, Debian). Each has unique sandboxing constraints that affect browser integration and smartcard access.

**Key Finding:** Snap/Flatpak can sandbox the desktop app, but the native messaging host MUST run unsandboxed for browser integration to work reliably. This requires a two-package approach:
1. Snap/Flatpak: Desktop app + daemon (sandboxed)
2. Debian package: Native host (system-wide, unsandboxed)

---

## Snap Constraints

### Current Snapcraft Configuration

**File:** [snap/snapcraft.yaml]

```yaml
name: openausweis
base: core24
confinement: strict

plugs:
  - desktop
  - desktop-legacy
  - wayland
  - x11
  - unity7
  - network
  - network-bind
  - home
  - raw-usb
  - hardware-observe
  - avahi-observe
```

### Analysis

#### ✅ Solvable: Daemon Socket Access

**Problem:** Snap daemon runs at `$SNAP_USER_DATA/openausweis/daemon.sock`; native host (outside snap) cannot access.

**Solution:** Use `$XDG_RUNTIME_DIR` instead.

**Implementation:**
- Daemon: If `$XDG_RUNTIME_DIR` is set, create socket at `$XDG_RUNTIME_DIR/openausweis/daemon.sock`
- Native host (outside snap): Same path
- Both processes can access: `/run/user/1000/openausweis/daemon.sock` (typical XDG_RUNTIME_DIR on Ubuntu)

**Verification:**
```bash
# Inside snap
ls -la $XDG_RUNTIME_DIR/openausweis/daemon.sock

# Outside snap (as user)
ls -la $XDG_RUNTIME_DIR/openausweis/daemon.sock

# Should be identical path and accessible by both
```

**Status:** ✅ Requires code change to daemon socket path logic.

---

#### ✅ Solvable: PC/SC Access

**Problem:** Snap is confined; can it access smartcard readers via pcscd?

**Solution:** pcscd runs as user service outside snap; snap daemon connects via Unix socket.

**Implementation:**
1. User installs pcscd: `sudo apt install pcscd libpcsclite1`
2. User starts pcscd as user service: `systemctl --user enable --now pcscd` (or auto-started by socket activation)
3. Snap daemon connects to pcscd socket: `/run/user/1000/pcscd/pcscd.comm` (or similar)
4. Snap has `raw-usb` plug: not needed if pcscd is running

**Configuration:**
- pcscd runs as `$USER` (not root)
- Socket created in `$XDG_RUNTIME_DIR`
- Snap user service can connect without special privileges

**Verification:**
```bash
# Check pcscd is running
systemctl --user status pcscd

# Check socket exists
ls -la /run/user/$(id -u)/pcscd/pcscd.comm

# Check snap daemon can connect
lsof -i | grep pcscd
```

**Status:** ✅ Requires user to start pcscd as user service (can be automated via systemd socket activation).

---

#### ⚠️ Critical Issue: Browser Native Messaging Isolation

**Problem:** Snap desktop app cannot directly communicate with unsandboxed browser via native messaging.

**Reason:**
1. Snap desktop app is confined; cannot directly execute unsandboxed native host binary
2. Browser extension (outside snap) cannot call snap-confined native host directly

**Scenario:**
```
Browser Extension (outside snap)
    ↓
    chrome.runtime.connectNative("org.openausweis.native")
    ↓
    System looks for manifest in:
    - /etc/chromium/native-messaging-hosts/
    - ~/.config/chromium/NativeMessagingHosts/
    ↓
    Manifest points to: /usr/bin/openausweis-native-host (unsandboxed)
    ✓ Works
    
    OR
    
    Manifest points to: /snap/openausweis/current/bin/openausweis-native-host (snapped)
    ✗ FAILS — snap-confined binary cannot be called as native host
```

**Solution: Separate Packages**
1. **Desktop app + daemon:** Distributed as Snap (sandboxed, desktop integration)
2. **Native host:** Distributed as separate Debian package (system-wide, unsandboxed)

**Implications:**
- User installs two packages:
  ```bash
  sudo apt install openausweis-native-host openausweis-daemon  # System packages
  sudo snap install openausweis  # Snap for desktop app
  ```
- Native host is optional for Snap-only users (browser integration won't work unless native host is installed)
- For Flatpak: same two-package approach

**Status:** ⚠️ Requires architectural decision: Ship native host separately.

---

#### Desktop Integration

**Plug: `desktop`, `desktop-legacy`**
- Snap can access GNOME/KDE desktop settings
- Tray icon support: Works via D-Bus (managed by Tauri)

**Plug: `wayland`, `x11`, `unity7`**
- Snap can run under Wayland and X11
- Tray icon positioning varies by DE

**Status:** ✅ No issues; Tauri handles cross-DE compatibility.

---

#### Network Access

**Plug: `network`, `network-bind`**
- Snap can bind to localhost ports
- Used for: REST API, WebSocket (desktop ↔ daemon)

**Status:** ✅ No issues.

---

### Snap Deployment Checklist

- [ ] **Separate packages:**
  - [ ] Desktop app + daemon in `openausweis` snap
  - [ ] Native host in `openausweis-native-host` Debian package
  
- [ ] **Socket path hardening:**
  - [ ] Daemon uses `$XDG_RUNTIME_DIR` for socket
  - [ ] Native host reads `$OPENAUSWEIS_DAEMON_SOCKET` or `$XDG_RUNTIME_DIR`
  
- [ ] **PC/SC integration:**
  - [ ] Document requirement: pcscd must run as user service
  - [ ] Add setup script: `scripts/setup-pcscd-user-service.sh`
  - [ ] Test on Ubuntu 24.04
  
- [ ] **Browser integration:**
  - [ ] Document requirement: browser MUST run outside snap
  - [ ] Ship native host Debian package separately
  - [ ] Create install docs
  
- [ ] **Testing:**
  - [ ] Snap build passes without errors
  - [ ] Desktop app launches from snap
  - [ ] Tray icon visible and responsive
  - [ ] Daemon socket is at correct path
  - [ ] Native host (unsandboxed) can reach daemon socket

---

## Flatpak Constraints

### Similar to Snap

**Permissions (equivalent):**
```
org.openausweis.openausweis:
  filesystems:
    - home
    - xdg-run/openausweis:create
  devices:
    - all
  system-talk:
    - org.freedesktop.DBus
```

**Issues:**
1. **Daemon socket path:** Use `$XDG_RUNTIME_DIR`
2. **PC/SC access:** Connect to unsandboxed pcscd via Unix socket
3. **Native host:** Must run unsandboxed (Debian package, same as Snap)

**Implementation:** Identical to Snap.

### Flatpak Deployment Checklist

- [ ] Same socket path changes as Snap
- [ ] Same PC/SC setup as Snap
- [ ] Same browser integration requirement (unsandboxed native host)
- [ ] Test on Ubuntu 24.04 with Flatpak runtime

---

## AppImage & Debian Package (No Sandboxing)

### No Constraints

**Architecture:**
```
User installs Debian packages:
  └── openausweis-daemon (system or user service)
  └── openausweis-native-host (system-wide binary)
  └── openausweis-desktop (GUI app)

No sandboxing; all components run with user privileges:
  ✓ Daemon can use any socket path
  ✓ Native host can reach daemon socket
  ✓ Browser can access native host
  ✓ pcscd accessible via any configured socket
```

**Installation:**
```bash
sudo apt install openausweis-daemon openausweis-native-host openausweis-desktop
```

**Status:** ✅ No issues.

---

## System Service Architecture

### Current: User-Facing App

**Deployment model (PHASE 2B):**
- Desktop app (Tauri) launched by user
- Daemon started via script or systemd user service
- Native host started via script or launched on-demand by extension

### Recommended: Systemd User Service

**For better reliability:**

**File:** `/usr/lib/systemd/user/openausweis-daemon.service`

```ini
[Unit]
Description=OpenAusweis Daemon
Documentation=https://github.com/openausweis/openausweis
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/openausweis-daemon
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
TimeoutStopSec=30

# Security
PrivateTmp=yes
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=%t/openausweis:%h/.config/openausweis:%h/.local/share/openausweis

[Install]
WantedBy=default.target
```

**Usage:**
```bash
# Enable and start
systemctl --user enable openausweis-daemon
systemctl --user start openausweis-daemon

# Check status
systemctl --user status openausweis-daemon

# View logs
journalctl --user -u openausweis-daemon -f
```

**Benefits:**
- Auto-restart on failure
- Clean logging to journal
- Survives desktop app crashes
- Can be controlled by package scripts

**Implementation:** Create service file during `apt install`.

---

## Browser Integration Roadmap

### PHASE 2B: Native Host Outside Sandbox

**Current:**
- Native host runs unsandboxed (Debian package)
- Browser extension connects to unsandboxed native host
- Native host reaches daemon socket in `$XDG_RUNTIME_DIR`
- Works for: AppImage, Debian, (Snap + unsandboxed native host), (Flatpak + unsandboxed native host)

**Limitation:** Browser must run outside snap/flatpak.

### PHASE 3: Improved Sandbox Integration

**Possible approaches:**
1. **Portal-based approach:** Use XDG Portal for unsandboxed process interaction
2. **Dbus service:** Daemon exposes Dbus interface; browser communicates via portal
3. **Browser extensions in sandbox:** If browsers support sandboxed extensions, integrate there

---

## Smartcard Reader Access

### PC/SC Architecture

**Standard setup:**
```
[Smartcard Reader (USB)]
    ↓
[pcscd daemon (listens on Unix socket)]
    ↓
[Client library (libpcsclite)]
```

**Current implementation:**
- Daemon uses [openausweis-pcsc](../../crates/openausweis-pcsc) crate
- Crate connects to pcscd via Unix socket
- Socket path: `/run/pcscd/pcscd.comm` (system service) or `$XDG_RUNTIME_DIR/pcscd/pcscd.comm` (user service)

### User Service Setup

**Problem:** System `pcscd` may not have appropriate permissions for snap/flatpak.

**Solution:** Run pcscd as user service.

**Installation script:** `scripts/setup-pcscd-user-service.sh` (to be created)

```bash
#!/bin/bash
# Enable pcscd user socket
systemctl --user enable --now pcscd.socket

# Verify
systemctl --user status pcscd.socket
systemctl --user list-sockets pcscd*
```

**How it works:**
1. pcscd.socket listens on `$XDG_RUNTIME_DIR/pcscd/pcscd.comm`
2. pcscd.service auto-starts when first client connects
3. Snap daemon can connect as user
4. No special privileges needed

---

## Security Considerations

### File Permissions

**Daemon socket:** `$XDG_RUNTIME_DIR/openausweis/daemon.sock`
- Mode: `0o600` (owner read-write only)
- Owner: `$USER`
- No access from other users (by default)

**Policy bundle:** `~/.config/openausweis/origin-policy/`
- Mode: `0o755` (world-readable)
- Owned by: `$USER`
- Contains no secrets (origin allowlist is public)

**Desktop app:** Runs as user (no privilege elevation)

### Privilege Model

**No sudo required** for normal operation:
- Smartcard reader access: Via pcscd (runs as user)
- Socket creation: In `$XDG_RUNTIME_DIR` (user-owned)
- Config access: In `~/.config` and `~/.local/share` (user-owned)

**Sudo used only for:**
- Snap/Flatpak installation
- System service installation (if opted)

---

## Known Issues & Limitations

### Snap/Flatpak Browser Integration

**Issue:** Browser inside snap/flatpak cannot directly access native host outside sandbox.

**Status:** ⚠️ Not supported in PHASE 2B. Browser must run outside snap/flatpak.

**Workaround:** Use system Firefox/Chromium (outside snap), not snap-packaged browser.

**Roadmap:** XDG Portal approach in PHASE 3.

### Multi-User Systems

**Issue:** Each user has separate `$XDG_RUNTIME_DIR` and pcscd socket; smartcard access is per-user.

**Scenario:**
- User A has smartcard reader attached
- User B cannot access card (runs separate daemon, separate pcscd socket)

**Status:** ✅ Expected behavior; each user has isolated authentication context.

### Session Switching (X11)

**Issue:** If user switches session (e.g., `Ctrl+Alt+F2`), daemon may continue running in background.

**Status:** ✅ Acceptable; daemon can be long-running across session switches.

### Wayland-Only Systems

**Issue:** Some desktop environments (GNOME 40+) strongly prefer Wayland; X11 compatibility needed for fallback.

**Status:** ✅ Tauri supports both; validate on both in testing phase.

---

## Ubuntu 24.04 Validation

### Minimum System Requirements

- OS: Ubuntu 24.04 LTS (or later)
- Desktop: GNOME 46+ (primary), KDE Plasma 5.27+ (secondary)
- Session: Wayland (primary), X11 (fallback)
- systemd: version 253+
- pcscd: libpcsclite1, pcscd (user service)

### Desktop Compatibility Matrix

| Desktop | Wayland | X11 | Tray | Notes |
|---------|---------|-----|------|-------|
| GNOME 46 | ✓ Primary | ✓ Fallback | AppIndicator | Use ayatana-appindicator |
| KDE Plasma 5.27 | ✓ | ✓ | Native | Statusnotifieritem protocol |

### Validation Script

**File:** [scripts/validate-linux-packaging.sh]

**Checks:**
- [ ] Ubuntu 24+ detected
- [ ] Desktop session identified (GNOME or KDE)
- [ ] Wayland session detected (primary requirement)
- [ ] X11 fallback available
- [ ] pcscd installed and runnable
- [ ] Daemon socket path writable
- [ ] Native messaging manifest installed (Chromium)
- [ ] Native messaging manifest installed (Firefox)
- [ ] Extension ID matches manifest

**Run:**
```bash
./scripts/validate-linux-packaging.sh \
  --expect-chromium-id ABCD1234... \
  --expect-firefox-id openausweis-browser@openausweis.org
```

---

## Deployment Checklist

### Snap

- [ ] Snapcraft.yaml configured correctly
- [ ] Daemon socket path uses `$XDG_RUNTIME_DIR`
- [ ] Desktop app runs from snap
- [ ] Tray icon visible
- [ ] pcscd user service setup documented
- [ ] Native host installed separately (Debian package)
- [ ] Browser extension can reach unsandboxed native host
- [ ] End-to-end authentication tested

### Flatpak

- [ ] Flatpak manifest configured
- [ ] Same checks as Snap
- [ ] Runtime compatibility verified

### Debian Package

- [ ] Installation scripts create systemd user service (optional)
- [ ] Man pages included
- [ ] Desktop file included
- [ ] Dependencies declared (pcscd, libpcsclite1, etc.)

### AppImage

- [ ] Bundled runtime compatible with Ubuntu 24.04
- [ ] pcscd must be installed system-wide
- [ ] Native host installed separately

---

**Document authored:** May 10, 2026  
**Review cycle:** Before PHASE 2B implementation  
**Owner:** DevOps / Release Engineering
