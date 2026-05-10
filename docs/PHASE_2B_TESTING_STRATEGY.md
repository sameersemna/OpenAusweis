# PHASE 2B: Testing Strategy

**Status:** Design Phase  
**Date:** May 10, 2026  
**Scope:** Unit, integration, E2E, system, security tests

---

## Testing Overview

**Goal:** Ensure PHASE 2B components (browser orchestration, origin validation, session lifecycle, Snap/Flatpak integration) work correctly and securely.

**Test Pyramid:**
```
                  ▲
                 / \
                /   \
              /  E2E  \
             /_________\
            /  System   \
           /  Integration \
          /_______________\
         /  Unit / Security \
        /_____________________\
```

**Coverage Target:** > 80% unit test coverage; 100% of critical paths exercised by integration tests.

---

## Unit Tests

### Daemon Session Manager

**File:** [apps/daemon/src/session.rs]

**Test Suite:**

#### test_start_session_creates_session_in_pin_entry
```rust
#[test]
fn start_session_creates_session_in_pin_entry() {
    let sessions = SessionManager::new(Duration::from_secs(60));
    let result = sessions.start_session(
        "https://example.bund.de".to_string(),
        Some("handoff-1".to_string()),
    );
    
    assert!(result.is_ok());
    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, SessionState::PinEntry);
    assert_eq!(sessions.active_count(), 1);
}
```

#### test_submit_pin_valid_transitions_to_card_interaction
```rust
#[test]
fn submit_pin_valid_transitions_to_card_interaction() {
    let sessions = SessionManager::new(Duration::from_secs(60));
    let start = sessions.start_session("https://site.bund.de".to_string(), None).unwrap();
    
    let result = sessions.submit_pin(start.session_id, "123456");
    assert!(result.is_ok());
    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, SessionState::CardInteraction);
}
```

#### test_submit_pin_invalid_format_rejects_and_counts
```rust
#[test]
fn submit_pin_invalid_format_rejects_and_counts() {
    let sessions = SessionManager::new(Duration::from_secs(60));
    let start = sessions.start_session("https://site.bund.de".to_string(), None).unwrap();
    
    // First invalid attempt
    let result1 = sessions.submit_pin(start.session_id, "12345");
    assert!(matches!(result1, Err(SubmitPinError::InvalidPinFormat { remaining_attempts: 2 })));
    
    // Second invalid attempt
    let result2 = sessions.submit_pin(start.session_id, "abcdef");
    assert!(matches!(result2, Err(SubmitPinError::InvalidPinFormat { remaining_attempts: 1 })));
    
    // Third invalid attempt
    let result3 = sessions.submit_pin(start.session_id, "");
    assert!(result3.is_err());
    
    // Session should be removed
    assert_eq!(sessions.active_count(), 0);
}
```

#### test_cancel_session_removes_session
```rust
#[test]
fn cancel_session_removes_session() {
    let sessions = SessionManager::new(Duration::from_secs(60));
    let start = sessions.start_session("https://site.bund.de".to_string(), None).unwrap();
    
    assert_eq!(sessions.active_count(), 1);
    assert!(sessions.cancel_session(start.session_id));
    assert_eq!(sessions.active_count(), 0);
}
```

#### test_prune_expired_removes_stale_sessions
```rust
#[test]
fn prune_expired_removes_stale_sessions() {
    let sessions = SessionManager::new(Duration::from_millis(100));
    let _ = sessions.start_session("https://site.bund.de".to_string(), None).unwrap();
    
    assert_eq!(sessions.active_count(), 1);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(sessions.active_count(), 0);
}
```

### Native Host Origin Validation

**File:** [apps/native-host/src/main.rs]

**Test Suite:**

#### test_load_origin_policy_from_bundle
```rust
#[test]
fn load_origin_policy_from_bundle() {
    // Create temp policy bundle
    let temp_dir = TempDir::new().unwrap();
    let policy_json = r#"{"allowed_exact_origins":["http://localhost"],"allowed_suffixes":[".bund.de"]}"#;
    std::fs::write(temp_dir.path().join("policy.json"), policy_json).unwrap();
    
    // Compute SHA256
    let digest = sha256::digest(policy_json);
    std::fs::write(temp_dir.path().join("policy.sha256"), format!("{}  policy.json", digest)).unwrap();
    
    // Test loading
    let policy = load_origin_policy(Some(temp_dir.path().to_string_lossy().to_string()));
    assert!(policy.allowed_exact_origins.contains(&"http://localhost".to_string()));
    assert!(policy.allowed_suffixes.contains(&".bund.de".to_string()));
}
```

#### test_is_allowed_origin_exact_match
```rust
#[test]
fn is_allowed_origin_exact_match() {
    let mut policy = OriginPolicy::default();
    policy.allowed_exact_origins.insert("https://exact.bund.de".to_string());
    
    assert!(is_allowed_origin("https://exact.bund.de", &policy));
    assert!(!is_allowed_origin("https://other.bund.de", &policy));
}
```

#### test_is_allowed_origin_suffix_match
```rust
#[test]
fn is_allowed_origin_suffix_match() {
    let mut policy = OriginPolicy::default();
    policy.allowed_suffixes.push(".bund.de".to_string());
    
    assert!(is_allowed_origin("https://any.bund.de", &policy));
    assert!(is_allowed_origin("https://sub.any.bund.de", &policy));
    assert!(!is_allowed_origin("https://bund.de", &policy)); // Exact match required for suffix
}
```

#### test_is_allowed_origin_rejects_unauthorized
```rust
#[test]
fn is_allowed_origin_rejects_unauthorized() {
    let policy = OriginPolicy::default(); // Empty allowlist
    
    assert!(!is_allowed_origin("https://evil.com", &policy));
    assert!(!is_allowed_origin("http://localhost:9999", &policy)); // Non-default port
}
```

### Extension Origin Validation

**File:** [apps/browser-extension/src/background.js]

**Test Suite (Jest):**

```javascript
describe("isAllowedOrigin", () => {
  test("accepts exact origin matches", () => {
    const policy = {
      exact: ["http://localhost", "https://localhost"],
      suffixes: [],
    };
    expect(isAllowedOrigin("http://localhost", policy)).toBe(true);
    expect(isAllowedOrigin("https://localhost", policy)).toBe(true);
  });

  test("accepts suffix matches", () => {
    const policy = {
      exact: [],
      suffixes: [".bund.de"],
    };
    expect(isAllowedOrigin("https://site.bund.de", policy)).toBe(true);
    expect(isAllowedOrigin("https://sub.site.bund.de", policy)).toBe(true);
  });

  test("rejects non-matching origins", () => {
    const policy = {
      exact: [],
      suffixes: [".bund.de"],
    };
    expect(isAllowedOrigin("https://evil.com", policy)).toBe(false);
    expect(isAllowedOrigin("http://localhost:9999", policy)).toBe(false);
  });
});
```

---

## Integration Tests

### Daemon + Native Host

**Scenario:** Native host forwards START_SESSION to daemon; receives SessionStarted.

**Setup:**
1. Start daemon: `./scripts/run-daemon.sh`
2. Start native host: `./scripts/run-native-host.sh`

**Test:**
```rust
#[tokio::test]
async fn test_native_host_start_session_flow() {
    let mut client = NativeHostClient::connect().await.unwrap();
    
    let request = RpcEnvelope {
        protocol_version: 1,
        request_id: uuid::Uuid::new_v4(),
        payload: ClientRequest::StartSession {
            relying_party: "https://localhost".to_string(),
            handoff_id: None,
        },
    };
    
    let response = client.send_request(&request).await.unwrap();
    
    match response.payload {
        DaemonResponse::SessionStarted { session_id, state, .. } => {
            assert_eq!(state, SessionState::PinEntry);
            assert!(!session_id.is_nil());
        }
        _ => panic!("unexpected response"),
    }
}
```

### Daemon + Desktop App

**Scenario:** Desktop app receives status updates; session lifecycle is observable.

**Setup:**
1. Build desktop: `npm run build --workspace @openausweis/desktop`
2. Start daemon
3. Launch desktop app

**Test:**
```rust
#[tokio::test]
async fn test_desktop_observes_session_lifecycle() {
    // Start daemon
    let daemon = DaemonProcess::spawn().await.unwrap();
    
    // Desktop connects to daemon
    let mut client = DaemonClient::connect().await.unwrap();
    
    // Receive initial status
    let status = client.get_status().await.unwrap();
    assert_eq!(status.active_session_count, 0);
    
    // Simulate START_SESSION (via another client)
    let another_client = DaemonClient::connect().await.unwrap();
    let session = another_client.start_session(
        "https://site.bund.de".to_string(),
        None,
    ).await.unwrap();
    
    // Desktop should receive updated status
    let updated_status = client.get_status_with_timeout(1000).await.unwrap();
    assert_eq!(updated_status.active_session_count, 1);
    
    // Simulate PIN submission
    another_client.submit_pin(session.session_id, "123456").await.ok();
    
    // Desktop should receive completion update
    let final_status = client.get_status_with_timeout(5000).await.unwrap();
    assert_eq!(final_status.active_session_count, 0); // Session completed and pruned
}
```

### Extension + Native Host

**Scenario:** Extension START_SESSION is forwarded to native host; extension watches session completion.

**Setup:**
1. Load extension as unpacked
2. Update native messaging manifest with extension ID
3. Start daemon and native host

**Test (using Puppeteer/Playwright for browser automation):**
```javascript
test("extension forwards start_session to native host", async () => {
  // Send message from extension background script
  const response = await browser.runtime.sendMessage({
    type: "START_SESSION",
    relying_party: "http://localhost:8080",
  });

  // Verify response structure
  expect(response).toHaveProperty("ok");
  expect(response.ok).toBe(true);
  expect(response.response).toHaveProperty("session_id");
  expect(response.response).toHaveProperty("state", "PIN_ENTRY");
});
```

---

## End-to-End Tests

### Full Browser Authentication Flow

**Scenario:** User visits localhost demo, clicks login, authenticates via desktop app, sees completion.

**Test Environment:**
- Daemon running
- Native host running
- Desktop app running
- Browser extension loaded (unpacked)
- Demo page running on `http://localhost:8080`

**Steps:**

```gherkin
Feature: Browser Authentication
  Scenario: User authenticates via demo page
    Given browser extension is loaded
    And daemon is running
    And native host is running
    And desktop app is running
    
    When user visits http://localhost:8080
    And user clicks "Login with OpenAusweis" button
    Then extension background receives START_SESSION
    And extension forwards to native host
    And native host forwards to daemon
    And daemon creates session in PinEntry state
    
    When desktop app shows session in status panel
    And user submits PIN (simulated: "123456")
    Then daemon transitions to CardInteraction
    And AuthExecutor (simulated/mock) completes
    Then daemon transitions to Completed
    
    When extension receives session completion via WATCH_SESSIONS
    And extension returns success to content script
    Then demo page receives openausweis:response event
    And demo page displays "Login successful"
```

**Automated Test (Playwright):**
```typescript
test("end-to-end browser authentication", async ({ browser }) => {
  const context = await browser.createBrowserContext();
  const page = await context.newPage();
  
  // Load extension background script
  const extensionId = await loadExtension(context, "./apps/browser-extension");
  
  // Visit demo page
  await page.goto("http://localhost:8080");
  
  // Click login button
  await page.click('[data-openausweis-login]');
  
  // Wait for response event
  const response = await page.waitForEvent("openausweis:response", { timeout: 30000 });
  
  // Verify success
  expect(response.detail).toHaveProperty("ok", true);
  expect(response.detail).toHaveProperty("response");
});
```

---

## System Tests

### Desktop Environment Compatibility

**Test Matrix:**
| OS | Desktop | Session | Test |
|----|---------|---------|----|
| Ubuntu 24.04 LTS | GNOME 46 | Wayland | ✓ Run validation script |
| Ubuntu 24.04 LTS | GNOME 46 | X11 | ✓ Run validation script |
| Ubuntu 24.04 LTS | KDE Plasma 5.27 | Wayland | ◯ If available |
| Ubuntu 24.04 LTS | KDE Plasma 5.27 | X11 | ◯ If available |

**Validation Script:** [scripts/validate-linux-packaging.sh]

**Checks:**
- OS is Linux, Ubuntu 24+
- Desktop session (GNOME or KDE)
- Session type (Wayland or X11)
- pcscd available
- Daemon socket path is writable
- Native messaging manifest is installed

**Run:**
```bash
./scripts/validate-linux-packaging.sh --expect-chromium-id ABCD... --expect-firefox-id openausweis@...
```

### Snap Build & Runtime

**Test Environment:** Ubuntu 24.04 with snapd

**Steps:**
1. Build snap: `snapcraft`
2. Install snap: `sudo snap install --dangerous openausweis_0.1.0_amd64.snap`
3. Launch desktop app: `openausweis` (from snap)
4. Verify tray icon appears
5. Verify daemon is running
6. Verify native host (unsandboxed) can reach daemon socket
7. Test browser extension authentication with snap daemon

**Expected:** All tests pass; no permission denied errors.

### Flatpak Build & Runtime

**Same as Snap; use Flatpak build system.**

---

## Security Tests

### Origin Validation Bypass Attempts

**Test:** Ensure malicious origins are rejected at each layer.

```rust
#[test]
fn test_malicious_origin_rejected_at_extension() {
    // Simulate malicious website sending START_SESSION
    let malicious_origin = "https://evil-attacker.com";
    let policy = load_default_policy();
    
    assert!(!is_allowed_origin(malicious_origin, &policy));
}

#[test]
fn test_malicious_origin_rejected_at_native_host() {
    let malicious_origin = "https://evil.com";
    
    // Native host should reject
    let is_allowed = is_allowed_origin_in_native_host(malicious_origin);
    assert!(!is_allowed);
    
    // Logs should record rejection
    let logs = read_native_host_logs();
    assert!(logs.contains("RP_NOT_ALLOWED"));
}
```

### PIN Attempt Limiting

**Test:** Ensure only 3 invalid PIN attempts are allowed.

```rust
#[test]
fn test_pin_attempt_limit_enforced() {
    let sessions = SessionManager::new(Duration::from_secs(60));
    let start = sessions.start_session("https://site.bund.de".to_string(), None).unwrap();
    
    // Attempt 1
    let _ = sessions.submit_pin(start.session_id, "000000");
    
    // Attempt 2
    let _ = sessions.submit_pin(start.session_id, "111111");
    
    // Attempt 3
    let result = sessions.submit_pin(start.session_id, "222222");
    assert!(matches!(result, Err(SubmitPinError::TooManyAttempts)));
    
    // Session should be gone
    assert_eq!(sessions.active_count(), 0);
}
```

### No Sensitive Data in Logs

**Test:** Verify PIN and session tokens are never logged.

```rust
#[test]
fn test_no_pin_in_logs() {
    env_logger::init();
    
    let sessions = SessionManager::new(Duration::from_secs(60));
    let start = sessions.start_session("https://site.bund.de".to_string(), None).unwrap();
    
    // Submit PIN
    let _ = sessions.submit_pin(start.session_id, "123456");
    
    // Capture logs
    let logs = capture_logs();
    
    // Verify PIN is not present
    assert!(!logs.contains("123456"));
    assert!(!logs.contains("pin"));
    assert!(!logs.contains("PIN"));
}
```

---

## Test Automation & CI/CD

### Local Testing

```bash
# Unit tests
npm run test                          # JS tests (Jest)
cargo test -p openausweis-daemon    # Rust tests (Cargo)
cargo test -p openausweis-native-host

# Integration tests
npm run test:integration

# E2E tests (requires running services)
npm run test:e2e
```

### GitHub Actions CI

**Trigger:** On PR and merge to main

**Matrix:**
- Ubuntu 24.04 LTS
- Rust stable
- Node.js 22

**Steps:**
1. Lint: `npm run lint`, `cargo fmt --check`
2. Type check: `npx tsc --noEmit`
3. Unit tests: `npm run test`, `cargo test`
4. Build: `npm run build`, `cargo build --release`
5. Packaging validation: `./scripts/validate-linux-packaging.sh`

**Duration:** ~15 minutes

**Report:** Coverage report uploaded to Codecov (goal: > 80% critical paths)

---

## Test Coverage Goals

| Component | Target | Current |
|-----------|--------|---------|
| Daemon session.rs | 90% | 0% (PHASE 2B) |
| Native host origin validation | 85% | 0% (PHASE 2B) |
| Extension background.js | 80% | 0% (PHASE 2B) |
| IPC protocol | 95% | 50% (PHASE 2A) |
| Error handling | 100% (critical paths) | 60% (PHASE 2A) |

---

## Known Limitations

### No E2E Browser Automation Yet

**Future:** Playwright/Puppeteer tests for browser extension can be flaky. Plan to stabilize in PHASE 3.

### No Hardware Smartcard Testing

**Current:** Tests use mock/simulated smartcard responses. Real smartcard testing is manual for now.

### No Performance Tests

**Future:** Add benchmarks for session creation, state transitions, native messaging latency.

---

**Document authored:** May 10, 2026  
**Ready for implementation:** Yes
