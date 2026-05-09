use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum OpenAusweisError {
    #[error("feature not yet implemented: {0}")]
    NotImplemented(&'static str),
}

#[derive(Debug, Clone)]
pub struct CardReaderSnapshot {
    pub name: String,
    pub card_present: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CardSubsystemSnapshot {
    pub pcsc_available: bool,
    pub readers: Vec<CardReaderSnapshot>,
    pub diagnostics: Vec<String>,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait SessionManager: Send + Sync {
    async fn start_session(&self, relying_party: &str) -> Result<Uuid, OpenAusweisError>;
    async fn cancel_session(&self, session_id: Uuid) -> Result<(), OpenAusweisError>;
}

#[async_trait]
pub trait CardSubsystem: Send + Sync {
    async fn snapshot(&self) -> CardSubsystemSnapshot;

    async fn is_pcsc_available(&self) -> bool {
        self.snapshot().await.pcsc_available
    }
}
