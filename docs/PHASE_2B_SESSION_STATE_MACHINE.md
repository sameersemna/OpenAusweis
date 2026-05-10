# PHASE 2B: Session State Machine & Lifecycle

**Status:** Design Phase  
**Date:** May 10, 2026  
**Scope:** Daemon session management, browser extension observation, state machine semantics

---

## Overview

The daemon's session state machine is the authoritative source of truth for authentication state. All clients (desktop UI, browser extension, native host) observe state changes; none can directly transition state.

**Key Design Principle:** Daemon owns the state; UI is read-only observer.

---

## State Machine Definition

### States

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  [START] → [PinEntry] → [CardInteraction] → [Completed]       │
│                              ↓                                  │
│                           [Error]                              │
│                              ↓                                  │
│                         [Expired/Cleaned Up]                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### State Enum

**File:** [openausweis-ipc/src/lib.rs]

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    #[serde(rename = "PIN_ENTRY")]
    PinEntry,
    
    #[serde(rename = "CARD_INTERACTION")]
    CardInteraction,
    
    #[serde(rename = "COMPLETED")]
    Completed,
    
    #[serde(rename = "ERROR")]
    Error,
}
```

### State Descriptions

| State | Meaning | User Action | Duration |
|-------|---------|-------------|----------|
| **PinEntry** | Waiting for user to submit 6-digit PIN | User enters PIN in desktop/modal | 1–300s (user-dependent) |
| **CardInteraction** | PIN accepted; authenticating with card via official stack | (Automatic) PACE/EAC negotiation | 2–10s (typical) |
| **Completed** | Authentication succeeded; session ready for relying party | Browser receives completion event | Remains visible for TTL |
| **Error** | Authentication failed; reason in error field | User dismisses error, retries | Remains visible for TTL |
| **(Expired)** | Not a real state; session removed from SessionManager after TTL | (N/A) | TTL = 5 minutes |

---

## State Transitions

### Transition Table

| From | To | Trigger | Condition | Action |
|------|----|---------|-----------|----|
| PinEntry | CardInteraction | SUBMIT_PIN | PIN valid (6 digits) | Update state, reset expiry, notify watch stream |
| PinEntry | Error | SUBMIT_PIN | PIN invalid (< 6 digits) | Increment pin_attempts, check < 3; if yes, return error; if no, remove session |
| CardInteraction | Completed | AuthExecutor::execute() succeeds | (Always) | Update state, reset expiry, notify watch stream |
| CardInteraction | Error | AuthExecutor::execute() fails | (Always) | Store error message, update state, reset expiry, notify watch stream |
| Any | (Expired) | Time expires | `now > entry.expires_at` | SessionManager::prune_expired removes session |

### Transition Validation

**File:** [apps/daemon/src/session.rs]

**Function:** `SessionManager::submit_pin(session_id, pin)`

```rust
pub fn submit_pin(&self, session_id: Uuid, pin: &str) -> Result<SessionSnapshot, SubmitPinError> {
    self.prune_expired();
    let mut guard = self.sessions.lock().expect("lock poisoned");
    
    let entry = match guard.get_mut(&session_id) {
        Some(entry) => entry,
        None => return Err(SubmitPinError::SessionNotFound),
    };
    
    // Validate PIN format
    if !is_valid_pin(pin) {  // Must be exactly 6 digits
        entry.pin_attempts = entry.pin_attempts.saturating_add(1);
        let remaining = 3_u8.saturating_sub(entry.pin_attempts);
        
        if entry.pin_attempts >= 3 {
            guard.remove(&session_id);  // Remove session on 3rd failed attempt
            return Err(SubmitPinError::TooManyAttempts);
        }
        
        return Err(SubmitPinError::InvalidPinFormat { remaining_attempts: remaining });
    }
    
    // PIN is valid; transition to CardInteraction
    entry.state = SessionState::CardInteraction;
    entry.last_error = None;
    entry.expires_at = Instant::now() + self.ttl;
    
    Ok(SessionSnapshot {
        session_id,
        state: entry.state,
        error: None,
        handoff_id: entry.handoff_id.clone(),
    })
}
```

---

## Session Lifecycle

### Complete Lifecycle (Happy Path)

**T0: START_SESSION**
```
Browser Extension → Native Host → Daemon
                                   ↓
                         Create session
                         State: PinEntry
                         TTL: Now + 5min
```

**T1: WATCH_SESSIONS (from browser)**
```
Browser Extension → Native Host → Daemon (stream begins)
                                   ↓
                    Send initial snapshot:
                    SessionUpdated {
                      session_id: "abcd-efgh-...",
                      state: PinEntry,
                      error: null,
                      handoff_id: "..."
                    }
```

**T2: (User inserts card, enters PIN via desktop app)**
```
Desktop App → Daemon (SUBMIT_PIN request)
              ↓
    Validate PIN format
    Transition to CardInteraction
    Launch AuthExecutor
    Return SessionUpdated to desktop
    
    WATCH_SESSIONS stream receives update:
    SessionUpdated {
      session_id: "abcd-efgh-...",
      state: CardInteraction,
      error: null
    }
```

**T3: (Official stack authenticates)**
```
AuthExecutor (via AusweisApp2 bridge)
    ↓
PACE protocol
EAC authentication
Response: ok=true
    ↓
Transition to Completed
Return SessionUpdated to desktop & watch stream
```

**T4: WATCH_SESSIONS (from browser) receives completion**
```
Daemon → Native Host → Browser Extension
                       ↓
                       Background script
                       resolves wait promise
                       Returns success to content script
```

**T5: (Session expires or is explicitly completed)**
```
SessionManager::prune_expired()
    ↓
Session removed from map after 5 min TTL
Desktop UI updates: active_session_count = 0
```

### Error Lifecycle

**T0: START_SESSION** → Session created, state=PinEntry

**T1: SUBMIT_PIN with invalid PIN (e.g., "12345")**
```
Daemon receives SUBMIT_PIN
PIN format check fails
Return error: SubmitPinError { InvalidPinFormat { remaining_attempts: 2 } }
Session state remains PinEntry
PIN attempts: 1/3
```

**T2: SUBMIT_PIN again with invalid PIN**
```
PIN attempt: 2/3
Return error with remaining_attempts: 1
```

**T3: SUBMIT_PIN third time with invalid PIN**
```
PIN attempt: 3/3 (exceeded)
Remove session from map
Return error: TooManyAttempts
WATCH_SESSIONS stream receives:
SessionCancelled { session_id: "abcd-efgh-..." }
Browser extension resolves wait promise with error
```

### AuthExecutor Error Lifecycle

**T0–T2:** PIN entry succeeds, transition to CardInteraction

**T3: AuthExecutor fails** (e.g., card not inserted)
```
AuthExecutor::execute() returns Err(anyhow!("Card not inserted"))

route_request processes error:
    session_manager.fail_session(session_id, error_message)
    ↓
SessionManager::fail_session:
    entry.state = SessionState::Error
    entry.last_error = Some("Card not inserted")
    entry.expires_at = Instant::now() + self.ttl
    
Return SessionUpdated { state: Error, error: Some(...) }
WATCH_SESSIONS stream sends update to browser
Browser extension receives error, returns to content script
Demo page displays: "Error: Card not inserted"
```

---

## Observability

### Desktop App Observation

**Method:** Long-lived WATCH_STATUS stream

**Endpoint:** Daemon WebSocket/IPC `WATCH_STATUS { interval_ms: 500 }`

**Observes:**
- `active_session_count: u32` — number of active sessions (0 or 1 for PHASE 2B)
- Tray tooltip updates: `"Session active (1)"` when count > 0
- Status panel shows session count in detail view

**Frequency:** Daemon publishes delta updates only (no change = no update)

**Latency:** ~500ms polling interval; user sees update within 1s

---

### Browser Extension Observation

**Method:** Long-lived WATCH_SESSIONS stream (per request_id)

**Endpoint:** Native host → Daemon WATCH_SESSIONS

**Observes:**
- Initial SessionUpdated or SessionCancelled snapshot
- State transitions: PinEntry → CardInteraction → Completed/Error
- Session metadata: session_id, handoff_id, error message

**Frequency:** Daemon publishes state changes only (initial + deltas)

**Latency:** < 1s typical (daemon notifies on state change, watch stream receives immediately)

---

### Daemon Logging

**File:** Structured tracing to stderr/logs

**Events:**
```
INFO: "session_start" session_id="abcd-..." state="PIN_ENTRY" relying_party="https://site.bund.de"
INFO: "session_transition" session_id="abcd-..." from_state="PIN_ENTRY" to_state="CARD_INTERACTION"
INFO: "session_complete" session_id="abcd-..." state="COMPLETED" duration_ms=3500
ERROR: "session_fail" session_id="abcd-..." error="Card not inserted" duration_ms=2100
DEBUG: "session_prune" removed_count=1 remaining=0
```

---

## Implementation Details

### SessionEntry (Internal Structure)

**File:** [apps/daemon/src/session.rs]

```rust
#[derive(Debug)]
struct SessionEntry {
    _relying_party: String,      // Audit: which site started this
    handoff_id: Option<String>,   // Optional browser-provided identifier
    state: SessionState,          // Current state
    pin_attempts: u8,             // Tracks invalid PIN attempts (0–3)
    last_error: Option<String>,   // Error message if state=Error
    expires_at: Instant,          // When this session is pruned
}
```

### SessionManager (Public API)

```rust
pub struct SessionManager {
    ttl: Duration,
    sessions: Mutex<HashMap<Uuid, SessionEntry>>,
}

impl SessionManager {
    pub fn new(ttl: Duration) -> Self { ... }
    
    pub fn prune_expired(&self) { ... }
    
    pub fn active_count(&self) -> u32 { ... }
    
    pub fn start_session(
        &self,
        relying_party: String,
        handoff_id: Option<String>,
    ) -> Result<SessionSnapshot, StartSessionError> { ... }
    
    pub fn cancel_session(&self, session_id: Uuid) -> bool { ... }
    
    pub fn current_session(&self) -> Option<SessionSnapshot> { ... }
    
    pub fn submit_pin(
        &self,
        session_id: Uuid,
        pin: &str,
    ) -> Result<SessionSnapshot, SubmitPinError> { ... }
    
    pub fn complete_session(&self, session_id: Uuid) -> Option<SessionSnapshot> { ... }
    
    pub fn fail_session(
        &self,
        session_id: Uuid,
        message: String,
    ) -> Option<SessionSnapshot> { ... }
}
```

### TTL & Cleanup

**TTL:** 5 minutes (300 seconds)

**Cleanup:** SessionManager::prune_expired() is called at the start of every operation:
- GET_STATUS
- START_SESSION
- SUBMIT_PIN
- CANCEL_SESSION
- WATCH_SESSIONS (before sending initial snapshot)

**Effect:** Sessions automatically removed when they expire; no background garbage collection needed.

---

## PHASE 2B Enhancements

### New: CANCEL_SESSION

**Currently Not Implemented:** Placeholder for future work.

**Expected Behavior:**
```
Request:
{
  "type": "CANCEL_SESSION",
  "session_id": "abcd-efgh-..."
}

Response (success):
{
  "type": "SESSION_CANCELLED",
  "session_id": "abcd-efgh-..."
}

Response (error):
{
  "type": "ERROR",
  "code": "SESSION_NOT_FOUND",
  "message": "session abcd-efgh-... not found"
}
```

**Daemon Logic:**
```rust
pub fn cancel_session(&self, session_id: Uuid) -> bool {
    self.prune_expired();
    let mut guard = self.sessions.lock().expect("lock poisoned");
    guard.remove(&session_id).is_some()
}
```

**When Triggered:**
- User clicks "Cancel" button in browser popup during active session
- Extension sends CANCEL_SESSION to native host
- Native host forwards to daemon
- Daemon removes session, returns SessionCancelled
- WATCH_SESSIONS stream receives SessionCancelled, resolves wait promise with cancellation

---

### New: Session History (Desktop Only)

**Concept:** Desktop app maintains a local log of last N completed/failed sessions.

**Storage:** `~/.local/share/openausweis/session-history.json`

**Format:**
```json
{
  "sessions": [
    {
      "session_id": "abcd-efgh-...",
      "relying_party": "https://site.bund.de",
      "state": "COMPLETED",
      "error": null,
      "started_at": "2026-05-10T14:30:00Z",
      "completed_at": "2026-05-10T14:30:03Z"
    },
    {
      "session_id": "xyz1-wxyz-...",
      "relying_party": "https://other.bund.de",
      "state": "ERROR",
      "error": "Card not inserted",
      "started_at": "2026-05-10T14:35:00Z",
      "completed_at": "2026-05-10T14:35:02Z"
    }
  ]
}
```

**Size Limit:** Keep last 100 sessions; older entries are rotated out.

**Use Case:** Diagnostics, debugging, audit trail (not security-critical for PHASE 2B).

---

### Enhanced Error Messages

**Current State:**
- Generic error messages: "PIN must be 6 digits"

**Enhanced:**
- PIN errors: "PIN must be exactly 6 digits (attempt 1/3)"
- Card errors: Specific reason from official stack (e.g., "PACE authentication failed", "EAC not supported")
- Timeout errors: "Authentication timed out; card may have been removed"

**Implementation:** Propagate error details from AuthExecutor through session state, surface in UI.

---

## Testing Checklist

### Unit Tests ([apps/daemon/src/session.rs])

- [ ] `test_start_session_creates_session_in_pin_entry`
- [ ] `test_submit_pin_invalid_format_rejects_and_counts`
- [ ] `test_submit_pin_three_invalid_attempts_removes_session`
- [ ] `test_submit_pin_valid_transitions_to_card_interaction`
- [ ] `test_cancel_session_removes_session`
- [ ] `test_complete_session_transitions_to_completed`
- [ ] `test_fail_session_transitions_to_error_with_message`
- [ ] `test_prune_expired_removes_stale_sessions`
- [ ] `test_active_count_returns_correct_count`
- [ ] `test_current_session_returns_only_active_session`
- [ ] `test_ttl_expiry_on_state_transition_resets_expiry`

### Integration Tests

- [ ] Daemon + Desktop: Full lifecycle PinEntry → CardInteraction → Completed
- [ ] Daemon + Desktop: Error path (card not inserted)
- [ ] Daemon + Browser Extension: WATCH_SESSIONS receives all state changes
- [ ] Daemon + Browser Extension: Session timeout (browser watch times out after 120s)
- [ ] Daemon + Browser Extension: Cancel session mid-flow

### System Tests

- [ ] Ubuntu 24.04, GNOME, Wayland
- [ ] Ubuntu 24.04, GNOME, X11
- [ ] Ubuntu 24.04, KDE, Wayland (if available)
- [ ] Session visible in tray tooltip
- [ ] Session visible in desktop status panel
- [ ] Session history persists (if implemented)

---

## FAQ

**Q: Can multiple sessions run in parallel?**  
A: No. Daemon enforces at most 1 active session. Second START_SESSION returns SESSION_ALREADY_ACTIVE.

**Q: What happens if a session expires while in CardInteraction?**  
A: Session is pruned from memory. Watch stream notices session gone, sends SessionCancelled. Browser extension receives cancellation, notifies user.

**Q: Can a malicious extension manipulate session state?**  
A: No. Extension can only observe state via WATCH_SESSIONS and submit PIN (if session in PinEntry). State transitions are enforced by daemon.

**Q: How does the desktop app know if a session is active?**  
A: Desktop polls daemon via WATCH_STATUS; active_session_count includes all active sessions.

**Q: What if the daemon crashes mid-authentication?**  
A: All sessions are lost (they're in-memory). Browser extension times out waiting for completion. User can retry after daemon restarts.

**Q: Can we persist sessions to disk?**  
A: Future enhancement (PHASE 3). For PHASE 2B, in-memory is sufficient.

---

**Document authored:** May 10, 2026  
**Ready for implementation:** Yes  
**Status:** Design complete
