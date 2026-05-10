use openausweis_ipc::SessionState;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub session_id: Uuid,
    pub state: SessionState,
    pub error: Option<String>,
    pub handoff_id: Option<String>,
}

#[derive(Debug)]
pub enum StartSessionError {
    SessionAlreadyActive,
}

impl std::fmt::Display for StartSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionAlreadyActive => write!(f, "a session is already active"),
        }
    }
}

impl std::error::Error for StartSessionError {}

#[derive(Debug)]
pub enum SubmitPinError {
    SessionNotFound,
    InvalidPinFormat { remaining_attempts: u8 },
    TooManyAttempts,
}

impl std::fmt::Display for SubmitPinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotFound => write!(f, "session not found"),
            Self::InvalidPinFormat { remaining_attempts } => write!(
                f,
                "PIN must be exactly 6 digits (remaining attempts: {remaining_attempts})"
            ),
            Self::TooManyAttempts => write!(f, "too many invalid PIN attempts"),
        }
    }
}

impl std::error::Error for SubmitPinError {}

#[derive(Debug)]
struct SessionEntry {
    _relying_party: String,
    handoff_id: Option<String>,
    state: SessionState,
    pin_attempts: u8,
    last_error: Option<String>,
    expires_at: Instant,
}

#[derive(Debug)]
pub struct SessionManager {
    ttl: Duration,
    sessions: Mutex<HashMap<Uuid, SessionEntry>>,
}

impl SessionManager {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn prune_expired(&self) {
        let mut guard = self.sessions.lock().expect("session lock poisoned");
        let now = Instant::now();
        guard.retain(|_, entry| entry.expires_at > now);
    }

    pub fn active_count(&self) -> u32 {
        self.prune_expired();
        let guard = self.sessions.lock().expect("session lock poisoned");
        guard.len() as u32
    }

    pub fn start_session(
        &self,
        relying_party: String,
        handoff_id: Option<String>,
    ) -> Result<SessionSnapshot, StartSessionError> {
        self.prune_expired();

        let mut guard = self.sessions.lock().expect("session lock poisoned");
        if !guard.is_empty() {
            return Err(StartSessionError::SessionAlreadyActive);
        }

        let session_id = Uuid::new_v4();
        let state = SessionState::PinEntry;
        let expires_at = Instant::now() + self.ttl;

        guard.insert(
            session_id,
            SessionEntry {
                _relying_party: relying_party,
                handoff_id: handoff_id.clone(),
                state,
                pin_attempts: 0,
                last_error: None,
                expires_at,
            },
        );

        Ok(SessionSnapshot {
            session_id,
            state,
            error: None,
            handoff_id,
        })
    }

    pub fn cancel_session(&self, session_id: Uuid) -> bool {
        self.prune_expired();
        let mut guard = self.sessions.lock().expect("session lock poisoned");
        guard.remove(&session_id).is_some()
    }

    pub fn current_session(&self) -> Option<SessionSnapshot> {
        self.prune_expired();
        let guard = self.sessions.lock().expect("session lock poisoned");
        guard.iter().next().map(|(session_id, entry)| SessionSnapshot {
            session_id: *session_id,
            state: entry.state,
            error: entry.last_error.clone(),
            handoff_id: entry.handoff_id.clone(),
        })
    }

    pub fn submit_pin(
        &self,
        session_id: Uuid,
        pin: &str,
    ) -> Result<SessionSnapshot, SubmitPinError> {
        self.prune_expired();
        let mut guard = self.sessions.lock().expect("session lock poisoned");

        let entry = match guard.get_mut(&session_id) {
            Some(entry) => entry,
            None => return Err(SubmitPinError::SessionNotFound),
        };

        if !is_valid_pin(pin) {
            entry.pin_attempts = entry.pin_attempts.saturating_add(1);
            let remaining_attempts = 3_u8.saturating_sub(entry.pin_attempts);
            if entry.pin_attempts >= 3 {
                guard.remove(&session_id);
                return Err(SubmitPinError::TooManyAttempts);
            }

            return Err(SubmitPinError::InvalidPinFormat { remaining_attempts });
        }

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

    pub fn complete_session(&self, session_id: Uuid) -> Option<SessionSnapshot> {
        self.prune_expired();
        let mut guard = self.sessions.lock().expect("session lock poisoned");
        let entry = guard.get_mut(&session_id)?;
        entry.state = SessionState::Completed;
        entry.expires_at = Instant::now() + self.ttl;

        Some(SessionSnapshot {
            session_id,
            state: SessionState::Completed,
            error: None,
            handoff_id: entry.handoff_id.clone(),
        })
    }

    pub fn fail_session(&self, session_id: Uuid, message: String) -> Option<SessionSnapshot> {
        self.prune_expired();
        let mut guard = self.sessions.lock().expect("session lock poisoned");
        let entry = guard.get_mut(&session_id)?;
        entry.state = SessionState::Error;
        entry.last_error = Some(message.clone());
        entry.expires_at = Instant::now() + self.ttl;

        Some(SessionSnapshot {
            session_id,
            state: SessionState::Error,
            error: Some(message),
            handoff_id: entry.handoff_id.clone(),
        })
    }

}

fn is_valid_pin(pin: &str) -> bool {
    pin.len() == 6 && pin.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_session_enforces_single_active_session() {
        let manager = SessionManager::new(Duration::from_secs(60));
        let first = manager
            .start_session("https://example.org".to_string(), Some("handoff-a".to_string()))
            .expect("first start should succeed");

        let second = manager.start_session("https://second.example.org".to_string(), None);
        assert!(matches!(
            second,
            Err(StartSessionError::SessionAlreadyActive)
        ));

        assert!(manager.cancel_session(first.session_id));

        let third = manager
            .start_session("https://third.example.org".to_string(), None)
            .expect("start after cancel should succeed");
        assert_eq!(third.state, SessionState::PinEntry);
        assert!(third.handoff_id.is_none());
    }

    #[test]
    fn submit_pin_enforces_format_and_completes_session() {
        let manager = SessionManager::new(Duration::from_secs(60));
        let session = manager
            .start_session("https://example.org".to_string(), Some("handoff-b".to_string()))
            .expect("start should succeed");

        let invalid = manager
            .submit_pin(session.session_id, "12")
            .expect_err("short PIN should fail");
        assert!(matches!(
            invalid,
            SubmitPinError::InvalidPinFormat {
                remaining_attempts: 2
            }
        ));

        let card_interaction = manager
            .submit_pin(session.session_id, "123456")
            .expect("valid PIN should complete placeholder flow");
        assert_eq!(card_interaction.state, SessionState::CardInteraction);
        assert!(card_interaction.error.is_none());
        assert_eq!(card_interaction.handoff_id.as_deref(), Some("handoff-b"));

        let completed = manager
            .complete_session(session.session_id)
            .expect("session should exist for completion");
        assert_eq!(completed.state, SessionState::Completed);
        assert!(completed.error.is_none());
        assert_eq!(completed.handoff_id.as_deref(), Some("handoff-b"));
    }

    #[test]
    fn fail_session_sets_error_state() {
        let manager = SessionManager::new(Duration::from_secs(60));
        let session = manager
            .start_session("https://example.org".to_string(), Some("handoff-c".to_string()))
            .expect("start should succeed");

        let failed = manager
            .fail_session(session.session_id, "executor failed".to_string())
            .expect("session should exist");
        assert_eq!(failed.state, SessionState::Error);
        assert_eq!(failed.error.as_deref(), Some("executor failed"));
        assert_eq!(failed.handoff_id.as_deref(), Some("handoff-c"));
    }
}
