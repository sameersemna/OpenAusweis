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
    StartSession {
        relying_party: String,
        handoff_id: Option<String>,
    },
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
        handoff_id: Option<String>,
    },
    SessionUpdated {
        session_id: Uuid,
        state: SessionState,
        error: Option<String>,
        handoff_id: Option<String>,
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
    #[serde(default)]
    pub ipc_diagnostics: IpcDiagnostics,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcDiagnostics {
    pub request_count: u64,
    pub error_count: u64,
    pub validation_rejections: u64,
    pub connection_failures: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderStatus {
    pub name: String,
    pub card_present: bool,
    pub error: Option<String>,
}
