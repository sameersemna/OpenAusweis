use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const IPC_PROTOCOL_VERSION: u16 = 1;

fn default_protocol_version() -> u16 {
    IPC_PROTOCOL_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcEnvelope<T> {
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u16,
    pub request_id: Uuid,
    pub payload: T,
}

impl<T> RpcEnvelope<T> {
    pub fn new(request_id: Uuid, payload: T) -> Self {
        Self {
            protocol_version: IPC_PROTOCOL_VERSION,
            request_id,
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientRequest {
    GetStatus,
    WatchStatus { interval_ms: u64 },
    WatchSessions { interval_ms: u64 },
    StartSession { relying_party: String },
    SubmitPin { session_id: Uuid, pin: String },
    CancelSession { session_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DaemonResponse {
    Status(DaemonStatus),
    SessionStarted {
        session_id: Uuid,
        state: SessionState,
    },
    SessionUpdated {
        session_id: Uuid,
        state: SessionState,
        error: Option<String>,
    },
    SessionCancelled { session_id: Uuid },
    Error { code: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionState {
    Idle,
    Active,
    PinEntry,
    CardInteraction,
    Completed,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub healthy: bool,
    pub pcsc_available: bool,
    pub active_session_count: u32,
    pub readers: Vec<ReaderStatus>,
    pub diagnostics: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderStatus {
    pub name: String,
    pub card_present: bool,
    pub error: Option<String>,
}
