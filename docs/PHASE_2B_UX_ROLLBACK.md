# PHASE 2B: Extension UX Recommendations & Rollback Strategy

**Status:** Design Phase  
**Date:** May 10, 2026  
**Scope:** Extension popup design, recovery UX, operational rollback procedures

---

## PART A: Extension UX Recommendations

### Popup Design Philosophy

**Principles:**
- **Minimal:** Only show critical information; hide details behind tabs/expandable sections
- **Informative:** Clearly communicate daemon health, reader status, active sessions
- **Recoverable:** Provide actionable next steps when things go wrong
- **Accessible:** WCAG 2.1 AA compliant; keyboard navigation, high contrast
- **Dark Mode:** Mandatory; follow system color scheme preference

---

### Layout & Components

#### 1. Header Section

```
┌─────────────────────────────────────┐
│ OpenAusweis              [↻ Refresh]│
└─────────────────────────────────────┘
```

**Elements:**
- Logo + title (left-aligned)
- Refresh button (right-aligned, keyboard shortcut: `Ctrl+Shift+R` / `Cmd+Shift+R`)

**Accessibility:**
- Button has `aria-label="Refresh daemon status"`
- Title has `<h1>` semantic heading

---

#### 2. Status Strip (Always Visible)

**Current:** 3-row layout for daemon, PC/SC, sessions status.

**Enhanced for PHASE 2B:**

```
┌─ Status ────────────────────────────────┐
│ ◉ Daemon     Connected                  │
│ ◉ PC/SC      Available                  │
│ ◌ Sessions   0 active                   │
└─────────────────────────────────────────┘
```

**Color Coding:**
- ◉ Green: OK
- ◌ Amber: Warning (1+ active sessions)
- ◌ Red: Error

**Line Items:**

| Label | Value | Color | Meaning |
|-------|-------|-------|---------|
| Daemon | Connected | 🟢 | Daemon responding |
| Daemon | Disconnected | 🔴 | Daemon not reachable |
| Daemon | Unhealthy | 🟠 | Daemon responding but degraded |
| PC/SC | Available | 🟢 | pcscd running, OK |
| PC/SC | Unavailable | 🟠 | pcscd not installed/running |
| Sessions | 0 active | ⚪ | No auth in progress |
| Sessions | 1 active | 🟡 | Auth in progress |

**Accessibility:**
- Text + color; color alone is not sufficient
- Each row has ARIA live region for updates: `aria-live="polite" aria-atomic="true"`

---

#### 3. Active Session Panel (Conditional)

**Shown only when `Sessions > 0`:**

```
┌─ Active Session ────────────────────┐
│ Origin:   https://site.bund.de      │
│ Started:  2:30:45 PM                │
│ Elapsed:  00:01:23                  │
│ State:    PIN_ENTRY                 │
│ [Cancel] [Dismiss]                  │
└─────────────────────────────────────┘
```

**Fields:**
- **Origin:** Relying party URL (truncated if long)
- **Started:** Time session was created (HH:MM:SS format)
- **Elapsed:** Time since session start (MM:SS format, updates every 1s)
- **State:** Current session state (PIN_ENTRY, CARD_INTERACTION, COMPLETED, ERROR)
- **Buttons:**
  - [Cancel]: Only enabled if CANCEL_SESSION is implemented; sends cancel request
  - [Dismiss]: Closes panel, does not cancel session

**Accessibility:**
- State is announced via live region when changes
- Elapsed time updates announced every 10s (to avoid excessive announcements)
- Buttons have descriptive labels

---

#### 4. Reader Status Panel

```
┌─ Readers ──────────────────────────────┐
│ 🔗 Readerware USB Smart Card Reader   │
│    🟢 Card present                    │
│                                       │
│ Or (if no readers):                  │
│ ℹ️  No readers detected. Attach a USB │
│    smartcard reader.                  │
└───────────────────────────────────────┘
```

**Items:**
- Reader name (with icon: 🔗 for connected)
- Card status (🟢 present, 🔴 absent)

**Conditional Messages:**
- If PC/SC unavailable: "Install and start pcscd to enable smartcard support."
- If PC/SC available but no readers: "Attach a USB smartcard reader."
- If readers present: List each reader with card status

**Accessibility:**
- Reader names have title attribute for full names (if truncated)
- Icons are decorative; text conveys meaning

---

#### 5. Diagnostics Panel (Expandable)

```
┌─ Bridge Diagnostics [⊕] ────────────┐
│ (Collapsed initially; click to expand)
└─────────────────────────────────────┘

[When expanded:]
┌─ Bridge Diagnostics [⊖] ────────────┐
│ ◉ Session starts        234          │
│ ◉ Completions          231          │
│ ◌ Watch retries          1          │
│ ◌ Native timeouts        0          │
│ ◌ Native disconnects     0          │
│ Last error: (None)                  │
│ Updated: 2:35:10 PM                 │
│ [Clear metrics]                     │
└─────────────────────────────────────┘
```

**Metrics:**
- Session starts: Total successful START_SESSION calls
- Completions: Sessions completed (success)
- Watch retries: WATCH_SESSIONS reconnect attempts
- Native timeouts: Native host did not respond in time
- Native disconnects: Native host connection lost
- Last error: Most recent error message (if any)
- Updated: Timestamp of last update
- [Clear metrics]: Button to reset counters (for debugging)

**Accessibility:**
- Expand/collapse: Keyboard accessible (Space/Enter on button)
- Metrics labeled with intent (not just numbers)

---

#### 6. Guidance Section (Conditional)

**Shown only when errors are detected:**

```
┌─ Guidance ───────────────────────────┐
│ ⚠️  Daemon is not running              │
│ Solution:                             │
│ 1. Open terminal                      │
│ 2. Run: ./scripts/run-daemon.sh      │
│ 3. Refresh this popup                 │
│ [Learn more] [Dismiss]                │
└─────────────────────────────────────┘
```

**Error Scenarios & Guidance:**

| State | Guidance |
|-------|----------|
| Daemon disconnected | Run `./scripts/run-daemon.sh` in terminal |
| PC/SC unavailable | Run `sudo systemctl start pcscd` or install libpcsclite |
| No readers detected | Attach a USB smartcard reader; run `pcsc_scan` to verify |
| Native host timeout | Ensure native messaging host is installed and started |
| Session error | Error message from daemon; check extension logs for details |

**Buttons:**
- [Learn more]: Link to DEVELOPMENT_WORKFLOW.md or FAQ
- [Dismiss]: Hides guidance section

---

### Visual Design

#### Color Scheme (Adaptive)

**Light Mode:**
- Background: `#f4f8fb` (light blue-gray)
- Surface: `#ffffff` (white)
- Text: `#182230` (dark blue-gray)
- Muted: `#5b6774` (gray)
- OK: `#1f6f2f` (dark green)
- Warn: `#806124` (dark amber)
- Error: `#8b2f2f` (dark red)

**Dark Mode (inverted):**
- Background: `#1a1f24` (dark gray-blue)
- Surface: `#252a30` (darker gray)
- Text: `#e0e6eb` (light gray-blue)
- Muted: `#a4adb5` (light gray)
- OK: `#6ab849` (bright green)
- Warn: `#e8cf98` (bright amber)
- Error: `#f5a8a8` (bright red)

**Badges:**
- Status indicators use color + icon + text
- Example: 🟢 Green dot + "Connected" text

#### Typography

- **Title:** 14px, 700 (bold)
- **Labels:** 12px, 600 (semibold), uppercase, letter-spacing 0.04em
- **Values:** 12px, 400 (regular)
- **Hints:** 11px, 400, muted color

#### Layout Grid

```
┌─────────────────────────────────┐
│ Header (40px)                   │
├─────────────────────────────────┤
│ Status Strip (90px)             │
├─────────────────────────────────┤
│ [Active Session] (if active)    │
├─────────────────────────────────┤
│ Readers Panel (60-120px)        │
├─────────────────────────────────┤
│ Diagnostics [+] (30px)          │
├─────────────────────────────────┤
│ [Guidance] (if error)           │
├─────────────────────────────────┘
```

**Popup Size:**
- Width: 280px (fixed)
- Height: 280–600px (dynamic based on content)

---

### Keyboard Navigation

**Tab Order:**
1. Refresh button
2. Cancel button (if session active)
3. Dismiss button (if session active)
4. Expand/collapse (diagnostics)
5. Clear metrics button (if diagnostics expanded)
6. Learn more / Dismiss (if guidance shown)

**Shortcuts:**
- `Ctrl+Shift+R` / `Cmd+Shift+R`: Refresh popup
- `Escape`: Close popup (browser standard)

---

### Accessibility Checklist

- [ ] WCAG 2.1 AA compliance (automated + manual testing)
- [ ] Color + text for all status indicators
- [ ] Minimum contrast ratio 4.5:1 (text on background)
- [ ] Focus indicators visible (outline: 2px solid)
- [ ] Live regions for status updates
- [ ] Semantic HTML: `<h1>`, `<button>`, `<section>`, etc.
- [ ] ARIA labels for icons and buttons
- [ ] Keyboard navigation complete (no mouse-only interactions)
- [ ] Screen reader tested (Orca on Ubuntu)

---

## PART B: Rollback Strategy

### Policy Bundle Rollback

**Scenario:** New origin policy is deployed but causes legitimate origins to be rejected.

**Detection:**
- Users report "origin not allowed" errors
- Logs show RP_NOT_ALLOWED for known-good origins

**Steps:**

1. **Identify last-known-good policy:**
   ```bash
   ls -la ~/.config/openausweis/origin-policy/policies/
   # Identify previous timestamp
   ```

2. **Revert symlink atomically:**
   ```bash
   cd ~/.config/openausweis/origin-policy
   ln -sfn policies/policy-PREVIOUS_TIMESTAMP/ current-tmp
   mv current-tmp current
   ```

3. **Verify checksum:**
   ```bash
   cd current
   sha256sum -c policy.sha256
   ```

4. **Test:**
   - Retry failed authentication
   - Monitor logs for RP_ALLOWED_ONLY_ALLOWED_SUFFIXES pattern

**Rollback Time:** < 1 minute

**Impact:** No restart needed; native host re-reads policy on next request.

---

### Extension Version Rollback

**Scenario:** New extension version has a bug (incorrect origin validation, etc.).

**Steps (for end users):**

1. **Chromium:**
   - Go to `chrome://extensions`
   - Disable broken extension
   - Download previous version from Chrome Web Store (or custom build)
   - Load as unpacked (if not in store)
   - Update native host manifest (if extension ID changed)

2. **Firefox:**
   - Go to `about:addons`
   - Click "manage" on broken add-on
   - Downgrade to previous version (if available in AMO history)
   - Or: Remove and reinstall from earlier release

**Steps (for developers/CI):**

1. **Revert commit:**
   ```bash
   git revert HEAD
   npm run build --workspace @openausweis/browser-extension
   ```

2. **Update manifest:**
   - If extension ID changed, update native host manifest

3. **Rebuild and redistribute:**
   - For Chromium: Upload new CRX to store
   - For Firefox: Resubmit to AMO

---

### Daemon Version Rollback

**Scenario:** New daemon version introduces session lifecycle bugs.

**Detection:**
- Sessions fail unexpectedly
- State transitions don't occur
- Logs show errors in route_request

**Steps (system-wide rollback):**

```bash
# Stop current daemon
systemctl --user stop openausweis-daemon

# Downgrade package
sudo apt install openausweis-daemon=VERSION-PREVIOUS

# Restart
systemctl --user start openausweis-daemon

# Verify
systemctl --user status openausweis-daemon
journalctl --user -u openausweis-daemon -f
```

**Alternative (if using systemd user service):**
```bash
# Revert to previous version and restart
sudo apt-get install --only-upgrade openausweis-daemon=0.1.0-prev
# Service auto-restarts due to Restart=on-failure
```

**Rollback Time:** ~ 30 seconds

**Impact:** All active sessions are lost (in-memory); users must retry authentication.

---

### Native Host Rollback

**Same as daemon:**

```bash
# Stop
systemctl --user stop openausweis-native-host  # If systemd service

# Or just kill
killall openausweis-native-host

# Downgrade
sudo apt install openausweis-native-host=VERSION-PREVIOUS

# Restart
systemctl --user start openausweis-native-host
```

---

### Snap Rollback

**Scenario:** Snap update breaks desktop app or daemon.

**Detection:**
- App fails to launch
- Tray icon missing
- Daemon crashes on start

**Steps (automatic via snapd):**

```bash
# List revisions
snap list --all openausweis

# Revert to previous revision
sudo snap revert openausweis

# Or: Revert to specific revision
sudo snap revert openausweis --revision=N
```

**How it works:**
- snapd keeps last 2–3 revisions on disk
- `snap revert` restores previous revision and restarts app
- No manual download needed

**Rollback Time:** ~ 10 seconds

---

### Flatpak Rollback

**Same as Snap:**

```bash
# List installations
flatpak list --app

# Rollback (depends on installation method)
# If installed from remote:
flatpak uninstall org.openausweis.openausweis//stable
flatpak install org.openausweis.openausweis//VERSION

# If installed from local file:
flatpak install openausweis-0.1.0-PREVIOUS.flatpak
```

---

### Total System Rollback

**Scenario:** Everything breaks; need to get back to known-good state.

**Steps:**

1. **Disable extension:**
   ```
   Chromium: chrome://extensions → toggle off
   Firefox: about:addons → manage → disable
   ```

2. **Stop daemon:**
   ```bash
   systemctl --user stop openausweis-daemon
   ```

3. **Stop native host:**
   ```bash
   killall openausweis-native-host
   ```

4. **Uninstall packages:**
   ```bash
   sudo apt remove openausweis-daemon openausweis-native-host openausweis-desktop
   ```

5. **Remove Snap/Flatpak (optional):**
   ```bash
   sudo snap remove openausweis
   flatpak uninstall org.openausweis.openausweis
   ```

6. **Clean up configuration:**
   ```bash
   rm -rf ~/.config/openausweis ~/.local/share/openausweis
   ```

**Rollback Time:** ~ 5 minutes

**Result:** System is back to pre-OpenAusweis state; no remnants.

---

### Verification Procedures

#### Rollback Verification Checklist

After any rollback, verify:

- [ ] **Daemon Status:**
  ```bash
  systemctl --user status openausweis-daemon
  journalctl --user -u openausweis-daemon -n 20
  ```
  Expected: `active (running)` with no recent errors

- [ ] **Native Host:**
  ```bash
  ps aux | grep openausweis-native-host
  ```
  Expected: Process is running (or will start on-demand)

- [ ] **Extension:**
  - Chromium: `chrome://extensions` → shows enabled extension
  - Firefox: `about:addons` → shows enabled add-on

- [ ] **Demo Page:**
  - Visit `http://localhost:8080`
  - Click "Login with OpenAusweis"
  - Verify popup shows "Daemon: Connected"

- [ ] **Logs:**
  - No recent errors in daemon logs
  - No native host timeouts
  - No origin validation failures

---

### Automated Rollback Testing

**Test Scenario:**
1. Deploy known-good version (baseline)
2. Deploy broken version
3. Execute automatic rollback
4. Verify system is restored to baseline

**CI/CD Integration:**
- Before merging to main, run rollback test
- If rollback fails, PR is rejected

---

### Troubleshooting Common Rollback Issues

| Issue | Cause | Fix |
|-------|-------|-----|
| Package downgrade fails | apt pinning or dependency conflict | Check `apt policy openausweis-daemon` |
| Snap revert unavailable | Revision not kept on disk | Manually download and install old version |
| Symlink revert fails | Permission denied | Ensure `$HOME/.config/openausweis` is writable |
| Extension ID mismatch after rollback | Extension version mismatches manifest | Update native host manifest or rebuild extension |
| Daemon socket stale | Previous daemon didn't clean up | Remove `$XDG_RUNTIME_DIR/openausweis/daemon.sock` manually |

---

### Release Checklist (Before Shipping)

- [ ] Rollback procedures documented in README
- [ ] Rollback tested locally (daemon, native host, extension, snap)
- [ ] CI/CD includes rollback verification test
- [ ] Policy bundle versioning scheme is clear (timestamp-based)
- [ ] Snap/Flatpak revisions kept for rollback (snapd/flatpak automatic)
- [ ] Users informed of how to downgrade (in docs)

---

**Document authored:** May 10, 2026  
**Ready for implementation:** Yes  
**Final document in PHASE 2B planning:** Yes
