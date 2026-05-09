use anyhow::{Context, Result};
use openausweis_core::CardSubsystem;
use openausweis_ipc::{
    ClientRequest, DaemonResponse, DaemonStatus, RpcEnvelope, IPC_PROTOCOL_VERSION,
};
use openausweis_pcsc::PcscSubsystem;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info};

const SOCKET_PATH: &str = "/tmp/openausweis-daemon.sock";

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    remove_stale_socket().await?;

    let listener = UnixListener::bind(SOCKET_PATH)
        .with_context(|| format!("failed to bind unix socket at {SOCKET_PATH}"))?;
    info!(socket = SOCKET_PATH, "daemon started");

    loop {
        let (stream, _) = listener.accept().await.context("accept failed")?;
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream).await {
                error!(error = %err, "connection error");
            }
        });
    }
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter("openausweis_daemon=info,openausweis=info")
        .with_target(false)
        .compact()
        .init();
}

async fn remove_stale_socket() -> Result<()> {
    match tokio::fs::remove_file(SOCKET_PATH).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).context("failed to remove stale socket"),
    }
}

async fn handle_connection(stream: UnixStream) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let card_subsystem = PcscSubsystem;

    while let Some(line) = lines.next_line().await.context("failed to read line")? {
        let request: RpcEnvelope<ClientRequest> =
            serde_json::from_str(&line).context("invalid request json")?;
        let response = if request.protocol_version == IPC_PROTOCOL_VERSION {
            route_request(request.payload, &card_subsystem).await
        } else {
            DaemonResponse::Error {
                code: "UNSUPPORTED_PROTOCOL".to_string(),
                message: format!(
                    "protocol {} is unsupported; expected {}",
                    request.protocol_version, IPC_PROTOCOL_VERSION
                ),
            }
        };

        let envelope = RpcEnvelope::new(request.request_id, response);

        let encoded = serde_json::to_string(&envelope).context("encode response failed")?;
        writer
            .write_all(encoded.as_bytes())
            .await
            .context("failed to write response")?;
        writer
            .write_all(b"\n")
            .await
            .context("failed to write response newline")?;
    }

    Ok(())
}

async fn route_request(
    request: ClientRequest,
    card_subsystem: &impl CardSubsystem,
) -> DaemonResponse {
    match request {
        ClientRequest::GetStatus => {
            let pcsc_available = card_subsystem.is_pcsc_available().await;
            DaemonResponse::Status(DaemonStatus {
                healthy: true,
                pcsc_available,
                active_session_count: 0,
            })
        }
        ClientRequest::StartSession { .. } => DaemonResponse::Error {
            code: "NOT_IMPLEMENTED".to_string(),
            message: "Session start orchestration not implemented yet".to_string(),
        },
        ClientRequest::CancelSession { session_id } => {
            DaemonResponse::SessionCancelled { session_id }
        }
    }
}
