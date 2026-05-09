use anyhow::{Context, Result};
use openausweis_core::CardSubsystem;
use openausweis_ipc::{
    ClientRequest, DaemonResponse, DaemonStatus, ReaderStatus, RpcEnvelope, IPC_PROTOCOL_VERSION,
};
use openausweis_pcsc::PcscSubsystem;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

const SOCKET_PATH: &str = "/tmp/openausweis-daemon.sock";
#[cfg(not(test))]
const WATCH_STATUS_MIN_INTERVAL_MS: u64 = 500;
#[cfg(test)]
const WATCH_STATUS_MIN_INTERVAL_MS: u64 = 20;

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    remove_stale_socket().await?;
    let card_subsystem: Arc<dyn CardSubsystem> = Arc::new(PcscSubsystem);

    let listener = UnixListener::bind(SOCKET_PATH)
        .with_context(|| format!("failed to bind unix socket at {SOCKET_PATH}"))?;
    info!(socket = SOCKET_PATH, "daemon started");

    loop {
        let (stream, _) = listener.accept().await.context("accept failed")?;
        let card_subsystem = Arc::clone(&card_subsystem);
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, &*card_subsystem).await {
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

async fn handle_connection(stream: UnixStream, card_subsystem: &dyn CardSubsystem) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await.context("failed to read line")? {
        let request: RpcEnvelope<ClientRequest> =
            serde_json::from_str(&line).context("invalid request json")?;
        if request.protocol_version != IPC_PROTOCOL_VERSION {
            let envelope = RpcEnvelope::new(
                request.request_id,
                DaemonResponse::Error {
                    code: "UNSUPPORTED_PROTOCOL".to_string(),
                    message: format!(
                        "protocol {} is unsupported; expected {}",
                        request.protocol_version, IPC_PROTOCOL_VERSION
                    ),
                },
            );
            write_envelope(&mut writer, &envelope).await?;
            continue;
        }

        if let ClientRequest::WatchStatus { interval_ms } = request.payload {
            stream_status_updates(request.request_id, interval_ms, card_subsystem, &mut writer)
                .await?;
            return Ok(());
        }

        let response = route_request(request.payload, card_subsystem).await;

        let envelope = RpcEnvelope::new(request.request_id, response);
        write_envelope(&mut writer, &envelope).await?;
    }

    Ok(())
}

async fn write_envelope(
    writer: &mut OwnedWriteHalf,
    envelope: &RpcEnvelope<DaemonResponse>,
) -> Result<()> {
    let encoded = serde_json::to_string(envelope).context("encode response failed")?;
    writer
        .write_all(encoded.as_bytes())
        .await
        .context("failed to write response")?;
    writer
        .write_all(b"\n")
        .await
        .context("failed to write response newline")?;
    Ok(())
}

async fn stream_status_updates(
    request_id: uuid::Uuid,
    interval_ms: u64,
    card_subsystem: &dyn CardSubsystem,
    writer: &mut OwnedWriteHalf,
) -> Result<()> {
    let interval = interval_ms.max(WATCH_STATUS_MIN_INTERVAL_MS);
    let mut last_status: Option<DaemonStatus> = None;

    loop {
        let status = build_daemon_status(card_subsystem).await;

        // Avoid spamming unchanged status snapshots over long-lived watch streams.
        if last_status.as_ref() == Some(&status) {
            sleep(Duration::from_millis(interval)).await;
            continue;
        }

        last_status = Some(status.clone());
        let envelope = RpcEnvelope::new(request_id, DaemonResponse::Status(status));

        if let Err(err) = write_envelope(writer, &envelope).await {
            info!(error = %err, "status watcher connection closed");
            return Ok(());
        }

        sleep(Duration::from_millis(interval)).await;
    }
}

async fn build_daemon_status(card_subsystem: &dyn CardSubsystem) -> DaemonStatus {
    let snapshot = card_subsystem.snapshot().await;
    let diagnostics = snapshot.diagnostics;
    let last_error = snapshot.last_error;

    if let Some(message) = &last_error {
        warn!(error = %message, "pcsc snapshot reported error");
    }

    DaemonStatus {
        healthy: true,
        pcsc_available: snapshot.pcsc_available,
        active_session_count: 0,
        readers: snapshot
            .readers
            .into_iter()
            .map(|reader| ReaderStatus {
                name: reader.name,
                card_present: reader.card_present,
                error: reader.error,
            })
            .collect(),
        diagnostics,
        last_error,
    }
}

async fn route_request(
    request: ClientRequest,
    card_subsystem: &dyn CardSubsystem,
) -> DaemonResponse {
    match request {
        ClientRequest::GetStatus => {
            DaemonResponse::Status(build_daemon_status(card_subsystem).await)
        }
        ClientRequest::WatchStatus { .. } => DaemonResponse::Error {
            code: "INVALID_REQUEST".to_string(),
            message: "WatchStatus must be handled by streaming connection path".to_string(),
        },
        ClientRequest::StartSession { .. } => DaemonResponse::Error {
            code: "NOT_IMPLEMENTED".to_string(),
            message: "Session start orchestration not implemented yet".to_string(),
        },
        ClientRequest::CancelSession { session_id } => {
            DaemonResponse::SessionCancelled { session_id }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use openausweis_core::{CardReaderSnapshot, CardSubsystemSnapshot};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct MockCardSubsystem {
        queue: Mutex<VecDeque<CardSubsystemSnapshot>>,
        fallback: CardSubsystemSnapshot,
    }

    impl MockCardSubsystem {
        fn from_snapshots(snapshots: Vec<CardSubsystemSnapshot>) -> Self {
            let fallback = snapshots.last().cloned().unwrap_or(CardSubsystemSnapshot {
                pcsc_available: false,
                readers: Vec::new(),
                diagnostics: vec!["mock-empty".to_string()],
                last_error: None,
            });

            Self {
                queue: Mutex::new(VecDeque::from(snapshots)),
                fallback,
            }
        }
    }

    #[async_trait]
    impl CardSubsystem for MockCardSubsystem {
        async fn snapshot(&self) -> CardSubsystemSnapshot {
            let mut guard = self.queue.lock().expect("queue lock poisoned");
            guard.pop_front().unwrap_or_else(|| self.fallback.clone())
        }
    }

    fn sample_snapshot(card_present: bool) -> CardSubsystemSnapshot {
        CardSubsystemSnapshot {
            pcsc_available: true,
            readers: vec![CardReaderSnapshot {
                name: "Mock Reader".to_string(),
                card_present,
                error: None,
            }],
            diagnostics: Vec::new(),
            last_error: None,
        }
    }

    #[tokio::test]
    async fn watch_stream_emits_initial_then_only_deltas() {
        let subsystem = MockCardSubsystem::from_snapshots(vec![
            sample_snapshot(false),
            sample_snapshot(false),
            sample_snapshot(true),
            sample_snapshot(true),
        ]);
        let request_id = uuid::Uuid::new_v4();

        let (reader_stream, writer_stream) = UnixStream::pair().expect("pair failed");
        let (_writer_reader, mut writer_half) = writer_stream.into_split();

        let task = tokio::spawn(async move {
            stream_status_updates(request_id, 1, &subsystem, &mut writer_half)
                .await
                .expect("stream failed");
        });

        let (reader_half, _reader_writer) = reader_stream.into_split();
        let mut lines = BufReader::new(reader_half).lines();

        let first = lines
            .next_line()
            .await
            .expect("read first line failed")
            .expect("missing first line");
        let second = lines
            .next_line()
            .await
            .expect("read second line failed")
            .expect("missing second line");

        let first_env: RpcEnvelope<DaemonResponse> =
            serde_json::from_str(&first).expect("parse first envelope failed");
        let second_env: RpcEnvelope<DaemonResponse> =
            serde_json::from_str(&second).expect("parse second envelope failed");

        assert_eq!(first_env.request_id, request_id);
        assert_eq!(second_env.request_id, request_id);

        let first_status = match first_env.payload {
            DaemonResponse::Status(status) => status,
            other => panic!("unexpected first payload: {other:?}"),
        };
        let second_status = match second_env.payload {
            DaemonResponse::Status(status) => status,
            other => panic!("unexpected second payload: {other:?}"),
        };

        assert!(!first_status.readers[0].card_present);
        assert!(second_status.readers[0].card_present);

        drop(lines);
        task.abort();
    }

    #[tokio::test]
    async fn route_request_rejects_watch_status_in_non_stream_path() {
        let subsystem = MockCardSubsystem::from_snapshots(vec![sample_snapshot(false)]);

        let response =
            route_request(ClientRequest::WatchStatus { interval_ms: 1000 }, &subsystem).await;

        match response {
            DaemonResponse::Error { code, message } => {
                assert_eq!(code, "INVALID_REQUEST");
                assert!(message.contains("streaming connection path"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_connection_returns_unsupported_protocol_error_envelope() {
        let subsystem = MockCardSubsystem::from_snapshots(vec![sample_snapshot(false)]);
        let (client_stream, server_stream) = UnixStream::pair().expect("pair failed");

        let server_task = tokio::spawn(async move {
            handle_connection(server_stream, &subsystem)
                .await
                .expect("handle_connection failed");
        });

        let (client_reader, mut client_writer) = client_stream.into_split();
        let request_id = uuid::Uuid::new_v4();
        let request = RpcEnvelope {
            protocol_version: IPC_PROTOCOL_VERSION + 1,
            request_id,
            payload: ClientRequest::GetStatus,
        };

        let encoded = serde_json::to_string(&request).expect("serialize request failed");
        client_writer
            .write_all(encoded.as_bytes())
            .await
            .expect("write request failed");
        client_writer
            .write_all(b"\n")
            .await
            .expect("write newline failed");
        drop(client_writer);

        let mut lines = BufReader::new(client_reader).lines();
        let response_line = lines
            .next_line()
            .await
            .expect("read response failed")
            .expect("missing response line");

        let response: RpcEnvelope<DaemonResponse> =
            serde_json::from_str(&response_line).expect("parse response failed");
        assert_eq!(response.protocol_version, IPC_PROTOCOL_VERSION);
        assert_eq!(response.request_id, request_id);

        match response.payload {
            DaemonResponse::Error { code, message } => {
                assert_eq!(code, "UNSUPPORTED_PROTOCOL");
                assert!(message.contains("unsupported"));
            }
            other => panic!("unexpected payload: {other:?}"),
        }

        server_task.await.expect("server task join failed");
    }
}
