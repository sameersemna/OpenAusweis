mod auth_executor;
mod session;

use auth_executor::AuthExecutor;
use anyhow::{Context, Result};
use openausweis_core::CardSubsystem;
use openausweis_ipc::{
    ClientRequest, DaemonResponse, DaemonStatus, IpcDiagnostics, ReaderStatus, RpcEnvelope,
    IPC_PROTOCOL_VERSION,
};
use openausweis_pcsc::PcscSubsystem;
use session::SessionManager;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration as StdDuration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{error, info, warn};

const SOCKET_PATH_FALLBACK: &str = "/tmp/openausweis-daemon.sock";
#[cfg(not(test))]
const WATCH_STATUS_MIN_INTERVAL_MS: u64 = 500;
#[cfg(test)]
const WATCH_STATUS_MIN_INTERVAL_MS: u64 = 20;
#[cfg(not(test))]
const WATCH_SESSIONS_MIN_INTERVAL_MS: u64 = 250;
#[cfg(test)]
const WATCH_SESSIONS_MIN_INTERVAL_MS: u64 = 20;
const SESSION_TTL_SECONDS: u64 = 5 * 60;

static LAST_LOGGED_PCSC_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[derive(Debug, Clone)]
struct DaemonDiagnostics {
    requests: u64,
    errors: u64,
    validation_rejections: u64,
}

impl DaemonDiagnostics {
    fn incr_request(&mut self) {
        self.requests = self.requests.saturating_add(1);
    }

    fn incr_error(&mut self) {
        self.errors = self.errors.saturating_add(1);
    }

    fn incr_validation_rejection(&mut self) {
        self.validation_rejections = self.validation_rejections.saturating_add(1);
    }

    fn to_ipc(&self) -> IpcDiagnostics {
        IpcDiagnostics {
            request_count: self.requests,
            error_count: self.errors,
            validation_rejections: self.validation_rejections,
            connection_failures: 0,
        }
    }
}

static DAEMON_DIAGNOSTICS: OnceLock<Mutex<DaemonDiagnostics>> = OnceLock::new();

fn get_diagnostics() -> DaemonDiagnostics {
    let lock = DAEMON_DIAGNOSTICS.get_or_init(|| {
        Mutex::new(DaemonDiagnostics {
            requests: 0,
            errors: 0,
            validation_rejections: 0,
        })
    });

    lock.lock().expect("diagnostics lock poisoned").clone()
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let socket_path = daemon_socket_path();
    validate_startup_environment(&socket_path)?;
    ensure_socket_parent_dir(&socket_path).await?;
    remove_stale_socket(&socket_path).await?;
    let card_subsystem: Arc<dyn CardSubsystem> = Arc::new(PcscSubsystem);
    let auth_executor = Arc::new(AuthExecutor::from_env());
    let session_manager = Arc::new(SessionManager::new(StdDuration::from_secs(
        SESSION_TTL_SECONDS,
    )));

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind unix socket at {}", socket_path.display()))?;
    lock_down_socket_permissions(&socket_path).await?;
    info!(socket = %socket_path.display(), "daemon started");

    // Set up signal handlers for graceful shutdown
    let mut sigterm =
        signal(SignalKind::terminate()).context("failed to set up SIGTERM handler")?;
    let mut sigint = signal(SignalKind::interrupt()).context("failed to set up SIGINT handler")?;

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (stream, _) = accept_result.context("accept failed")?;
                let card_subsystem = Arc::clone(&card_subsystem);
                let auth_executor = Arc::clone(&auth_executor);
                let session_manager = Arc::clone(&session_manager);
                tokio::spawn(async move {
                    if let Err(err) =
                        handle_connection(stream, &*card_subsystem, &auth_executor, &session_manager).await
                    {
                        error!(error = %err, "connection error");
                    }
                });
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM, initiating graceful shutdown");
                break;
            }
            _ = sigint.recv() => {
                info!("received SIGINT, initiating graceful shutdown");
                break;
            }
        }
    }

    // Clean up: close listener and remove socket file
    drop(listener);
    remove_stale_socket(&socket_path).await?;
    info!(socket = %socket_path.display(), "daemon shut down cleanly");

    Ok(())
}

async fn lock_down_socket_permissions(socket_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        tokio::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))
            .await
            .with_context(|| {
                format!(
                    "failed to lock down daemon socket permissions at {}",
                    socket_path.display()
                )
            })?;
    }

    Ok(())
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter("openausweis_daemon=info,openausweis=info")
        .with_target(false)
        .compact()
        .init();
}

fn validate_startup_environment(socket_path: &Path) -> Result<()> {
    // Verify socket path is in a safe location (XDG_RUNTIME_DIR or /tmp)
    let socket_str = socket_path.to_string_lossy().to_string();
    let xdg_runtime = std::env::var("XDG_RUNTIME_DIR").ok();
    let is_in_runtime_dir = xdg_runtime
        .as_ref()
        .map(|rt| socket_str.starts_with(rt))
        .unwrap_or(false);
    let is_in_tmp = socket_str.starts_with("/tmp/");

    if !is_in_runtime_dir && !is_in_tmp {
        warn!(
            socket = %socket_path.display(),
            xdg_runtime = ?xdg_runtime,
            "socket path is not in XDG_RUNTIME_DIR or /tmp; portability may be affected"
        );
    }

    // Verify socket parent directory can be created (early check)
    if let Some(parent) = socket_path.parent() {
        if parent.as_os_str().is_empty() {
            return Err(anyhow::anyhow!("invalid socket path parent"));
        }
    }

    info!(
        socket = %socket_path.display(),
        xdg_runtime_dir = ?xdg_runtime,
        "startup validation: socket path is acceptable"
    );

    Ok(())
}

async fn remove_stale_socket(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).context("failed to remove stale socket"),
    }
}

fn daemon_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("OPENAUSWEIS_DAEMON_SOCKET") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !runtime_dir.trim().is_empty() {
            return PathBuf::from(runtime_dir)
                .join("openausweis")
                .join("daemon.sock");
        }
    }

    PathBuf::from(SOCKET_PATH_FALLBACK)
}

async fn ensure_socket_parent_dir(socket_path: &Path) -> Result<()> {
    let Some(parent) = socket_path.parent() else {
        return Ok(());
    };

    tokio::fs::create_dir_all(parent)
        .await
        .with_context(|| format!("failed to create daemon socket parent at {}", parent.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .await
            .with_context(|| {
                format!(
                    "failed to lock down daemon socket parent permissions at {}",
                    parent.display()
                )
            })?;
    }

    Ok(())
}

async fn handle_connection(
    stream: UnixStream,
    card_subsystem: &dyn CardSubsystem,
    auth_executor: &AuthExecutor,
    session_manager: &SessionManager,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await.context("failed to read line")? {
        let request: RpcEnvelope<ClientRequest> =
            serde_json::from_str(&line).context("invalid request json")?;

        {
            let lock = DAEMON_DIAGNOSTICS.get_or_init(|| {
                Mutex::new(DaemonDiagnostics {
                    requests: 0,
                    errors: 0,
                    validation_rejections: 0,
                })
            });
            let mut diag = lock.lock().expect("diagnostics lock poisoned");
            diag.incr_request();
        }

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

            {
                let lock = DAEMON_DIAGNOSTICS.get_or_init(|| {
                    Mutex::new(DaemonDiagnostics {
                        requests: 0,
                        errors: 0,
                        validation_rejections: 0,
                    })
                });
                let mut diag = lock.lock().expect("diagnostics lock poisoned");
                diag.incr_error();
                diag.incr_validation_rejection();
            }

            write_envelope(&mut writer, &envelope).await?;
            continue;
        }

        if let ClientRequest::WatchStatus { interval_ms } = request.payload {
            stream_status_updates(
                request.request_id,
                interval_ms,
                card_subsystem,
                session_manager,
                &mut writer,
            )
            .await?;
            return Ok(());
        }

        if let ClientRequest::WatchSessions { interval_ms } = request.payload {
            stream_session_updates(request.request_id, interval_ms, session_manager, &mut writer)
                .await?;
            return Ok(());
        }

        let response =
            route_request(request.payload, card_subsystem, auth_executor, session_manager).await;

        if matches!(response, DaemonResponse::Error { .. }) {
            let lock = DAEMON_DIAGNOSTICS.get_or_init(|| {
                Mutex::new(DaemonDiagnostics {
                    requests: 0,
                    errors: 0,
                    validation_rejections: 0,
                })
            });
            let mut diag = lock.lock().expect("diagnostics lock poisoned");
            diag.incr_error();
        }

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
    session_manager: &SessionManager,
    writer: &mut OwnedWriteHalf,
) -> Result<()> {
    let interval = interval_ms.max(WATCH_STATUS_MIN_INTERVAL_MS);
    let mut last_status: Option<DaemonStatus> = None;

    loop {
        let status = build_daemon_status(card_subsystem, session_manager).await;

        // Avoid spamming unchanged status snapshots over long-lived watch streams.
        if last_status.as_ref() == Some(&status) {
            sleep(TokioDuration::from_millis(interval)).await;
            continue;
        }

        last_status = Some(status.clone());
        let envelope = RpcEnvelope::new(request_id, DaemonResponse::Status(status));

        if let Err(err) = write_envelope(writer, &envelope).await {
            info!(error = %err, "status watcher connection closed");
            return Ok(());
        }

        sleep(TokioDuration::from_millis(interval)).await;
    }
}

async fn stream_session_updates(
    request_id: uuid::Uuid,
    interval_ms: u64,
    session_manager: &SessionManager,
    writer: &mut OwnedWriteHalf,
) -> Result<()> {
    let interval = interval_ms.max(WATCH_SESSIONS_MIN_INTERVAL_MS);
    let mut last_snapshot = session_manager.current_session();

    if let Some(snapshot) = last_snapshot.clone() {
        let envelope = RpcEnvelope::new(
            request_id,
            DaemonResponse::SessionUpdated {
                session_id: snapshot.session_id,
                state: snapshot.state,
                error: snapshot.error.clone(),
                handoff_id: snapshot.handoff_id.clone(),
            },
        );

        write_envelope(writer, &envelope).await?;
    }

    loop {
        let current_snapshot = session_manager.current_session();

        if current_snapshot != last_snapshot {
            match (&last_snapshot, &current_snapshot) {
                (Some(previous), None) => {
                    let envelope = RpcEnvelope::new(
                        request_id,
                        DaemonResponse::SessionCancelled {
                            session_id: previous.session_id,
                        },
                    );
                    if let Err(err) = write_envelope(writer, &envelope).await {
                        info!(error = %err, "session watcher connection closed");
                        return Ok(());
                    }
                }
                (_, Some(current)) => {
                    let envelope = RpcEnvelope::new(
                        request_id,
                        DaemonResponse::SessionUpdated {
                            session_id: current.session_id,
                            state: current.state,
                            error: current.error.clone(),
                            handoff_id: current.handoff_id.clone(),
                        },
                    );
                    if let Err(err) = write_envelope(writer, &envelope).await {
                        info!(error = %err, "session watcher connection closed");
                        return Ok(());
                    }
                }
                (None, None) => {}
            }

            last_snapshot = current_snapshot;
        }

        sleep(TokioDuration::from_millis(interval)).await;
    }
}

async fn build_daemon_status(
    card_subsystem: &dyn CardSubsystem,
    session_manager: &SessionManager,
) -> DaemonStatus {
    let snapshot = card_subsystem.snapshot().await;
    let diagnostics = snapshot.diagnostics;
    let last_error = snapshot.last_error;

    log_pcsc_error_if_changed(&last_error);

    let ipc_diagnostics = get_diagnostics().to_ipc();

    DaemonStatus {
        healthy: true,
        pcsc_available: snapshot.pcsc_available,
        active_session_count: session_manager.active_count(),
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
        ipc_diagnostics,
    }
}

fn log_pcsc_error_if_changed(last_error: &Option<String>) {
    let lock = LAST_LOGGED_PCSC_ERROR.get_or_init(|| Mutex::new(None));
    let mut previous = lock.lock().expect("pcsc error log state lock poisoned");

    if !should_log_pcsc_error_transition(&previous, last_error) {
        return;
    }

    match last_error {
        Some(message) => warn!(error = %message, "pcsc snapshot reported error"),
        None => info!("pcsc snapshot error cleared"),
    }

    *previous = last_error.clone();
}

fn should_log_pcsc_error_transition(previous: &Option<String>, current: &Option<String>) -> bool {
    previous != current
}

async fn route_request(
    request: ClientRequest,
    card_subsystem: &dyn CardSubsystem,
    auth_executor: &AuthExecutor,
    session_manager: &SessionManager,
) -> DaemonResponse {
    match request {
        ClientRequest::GetStatus => {
            DaemonResponse::Status(build_daemon_status(card_subsystem, session_manager).await)
        }
        ClientRequest::WatchStatus { .. } => DaemonResponse::Error {
            code: "INVALID_REQUEST".to_string(),
            message: "WatchStatus must be handled by streaming connection path".to_string(),
        },
        ClientRequest::WatchSessions { .. } => DaemonResponse::Error {
            code: "INVALID_REQUEST".to_string(),
            message: "WatchSessions must be handled by streaming connection path".to_string(),
        },
        ClientRequest::StartSession {
            relying_party,
            handoff_id,
        } => {
            match session_manager.start_session(relying_party, handoff_id) {
                Ok(session) => DaemonResponse::SessionStarted {
                    session_id: session.session_id,
                    state: session.state,
                    handoff_id: session.handoff_id,
                },
                Err(err) => DaemonResponse::Error {
                    code: "SESSION_ALREADY_ACTIVE".to_string(),
                    message: err.to_string(),
                },
            }
        }
        ClientRequest::SubmitPin { session_id, pin } => {
            match session_manager.submit_pin(session_id, &pin) {
                Ok(_) => {
                    match auth_executor.execute(session_id).await {
                        Ok(()) => match session_manager.complete_session(session_id) {
                            Some(session) => DaemonResponse::SessionUpdated {
                                session_id: session.session_id,
                                state: session.state,
                                error: session.error,
                                handoff_id: session.handoff_id,
                            },
                            None => DaemonResponse::Error {
                                code: "SESSION_NOT_FOUND".to_string(),
                                message: format!("session not found: {session_id}"),
                            },
                        },
                        Err(err) => {
                            let message = err.to_string();
                            match session_manager.fail_session(session_id, message.clone()) {
                                Some(session) => DaemonResponse::SessionUpdated {
                                    session_id: session.session_id,
                                    state: session.state,
                                    error: session.error,
                                    handoff_id: session.handoff_id,
                                },
                                None => DaemonResponse::Error {
                                    code: "SESSION_NOT_FOUND".to_string(),
                                    message: format!("session not found: {session_id}"),
                                },
                            }
                        }
                    }
                }
                Err(err) => DaemonResponse::Error {
                    code: "INVALID_PIN".to_string(),
                    message: err.to_string(),
                },
            }
        }
        ClientRequest::CancelSession { session_id } => {
            if session_manager.cancel_session(session_id) {
                DaemonResponse::SessionCancelled { session_id }
            } else {
                DaemonResponse::Error {
                    code: "SESSION_NOT_FOUND".to_string(),
                    message: format!("session not found: {session_id}"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use openausweis_core::{CardReaderSnapshot, CardSubsystemSnapshot};
    use openausweis_ipc::SessionState;
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

    fn test_sessions() -> SessionManager {
        SessionManager::new(StdDuration::from_secs(60))
    }

    fn test_auth_executor() -> AuthExecutor {
        AuthExecutor::mock()
    }

    fn failing_auth_executor() -> AuthExecutor {
        AuthExecutor::fail_for_tests("forced executor failure")
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
            let sessions = test_sessions();
            stream_status_updates(request_id, 1, &subsystem, &sessions, &mut writer_half)
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
        let executor = test_auth_executor();
        let sessions = test_sessions();

        let response = route_request(
            ClientRequest::WatchStatus { interval_ms: 1000 },
            &subsystem,
            &executor,
            &sessions,
        )
        .await;

        match response {
            DaemonResponse::Error { code, message } => {
                assert_eq!(code, "INVALID_REQUEST");
                assert!(message.contains("streaming connection path"));
            }
            other => panic!("unexpected response: {other:?}"),
        }

        let watch_sessions_response = route_request(
            ClientRequest::WatchSessions { interval_ms: 1000 },
            &subsystem,
            &executor,
            &sessions,
        )
        .await;

        match watch_sessions_response {
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
        let executor = test_auth_executor();
        let sessions = test_sessions();
        let (client_stream, server_stream) = UnixStream::pair().expect("pair failed");

        let server_task = tokio::spawn(async move {
            handle_connection(server_stream, &subsystem, &executor, &sessions)
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

    #[tokio::test]
    async fn route_request_start_session_creates_active_session() {
        let subsystem = MockCardSubsystem::from_snapshots(vec![sample_snapshot(false)]);
        let executor = test_auth_executor();
        let sessions = test_sessions();

        let response = route_request(
            ClientRequest::StartSession {
                relying_party: "https://localhost".to_string(),
                handoff_id: Some("handoff-test-1".to_string()),
            },
            &subsystem,
            &executor,
            &sessions,
        )
        .await;

        match response {
            DaemonResponse::SessionStarted {
                session_id,
                state,
                handoff_id,
            } => {
                assert_eq!(state, SessionState::PinEntry);
                assert_eq!(handoff_id.as_deref(), Some("handoff-test-1"));
                assert_eq!(sessions.active_count(), 1);
                let snapshot = sessions
                    .current_session()
                    .expect("current session should exist");
                assert_eq!(snapshot.session_id, session_id);
                assert_eq!(snapshot.handoff_id.as_deref(), Some("handoff-test-1"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn route_request_submit_pin_completes_session() {
        let subsystem = MockCardSubsystem::from_snapshots(vec![sample_snapshot(false)]);
        let executor = test_auth_executor();
        let sessions = test_sessions();

        let started = route_request(
            ClientRequest::StartSession {
                relying_party: "https://localhost".to_string(),
                handoff_id: Some("handoff-test-2".to_string()),
            },
            &subsystem,
            &executor,
            &sessions,
        )
        .await;

        let session_id = match started {
            DaemonResponse::SessionStarted { session_id, .. } => session_id,
            other => panic!("unexpected start response: {other:?}"),
        };

        let submitted = route_request(
            ClientRequest::SubmitPin {
                session_id,
                pin: "123456".to_string(),
            },
            &subsystem,
            &executor,
            &sessions,
        )
        .await;

        match submitted {
            DaemonResponse::SessionUpdated {
                session_id: returned,
                state,
                error,
                handoff_id,
            } => {
                assert_eq!(returned, session_id);
                assert_eq!(state, SessionState::Completed);
                assert!(error.is_none());
                assert_eq!(handoff_id.as_deref(), Some("handoff-test-2"));
            }
            other => panic!("unexpected submit response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn route_request_submit_pin_executor_failure_sets_error_state() {
        let subsystem = MockCardSubsystem::from_snapshots(vec![sample_snapshot(false)]);
        let executor = failing_auth_executor();
        let sessions = test_sessions();

        let started = route_request(
            ClientRequest::StartSession {
                relying_party: "https://localhost".to_string(),
                handoff_id: Some("handoff-test-3".to_string()),
            },
            &subsystem,
            &executor,
            &sessions,
        )
        .await;

        let session_id = match started {
            DaemonResponse::SessionStarted { session_id, .. } => session_id,
            other => panic!("unexpected start response: {other:?}"),
        };

        let submitted = route_request(
            ClientRequest::SubmitPin {
                session_id,
                pin: "123456".to_string(),
            },
            &subsystem,
            &executor,
            &sessions,
        )
        .await;

        match submitted {
            DaemonResponse::SessionUpdated {
                session_id: returned,
                state,
                error,
                handoff_id,
            } => {
                assert_eq!(returned, session_id);
                assert_eq!(state, SessionState::Error);
                assert_eq!(error.as_deref(), Some("forced executor failure"));
                assert_eq!(handoff_id.as_deref(), Some("handoff-test-3"));
            }
            other => panic!("unexpected submit response: {other:?}"),
        }
    }

    #[tokio::test]
    async fn watch_sessions_emits_start_and_cancel_deltas() {
        let sessions = std::sync::Arc::new(SessionManager::new(StdDuration::from_secs(60)));
        let (reader_stream, writer_stream) = UnixStream::pair().expect("pair failed");
        let (_writer_reader, mut writer_half) = writer_stream.into_split();
        let request_id = uuid::Uuid::new_v4();

        let sessions_for_stream = std::sync::Arc::clone(&sessions);
        let sessions_for_updates = std::sync::Arc::clone(&sessions_for_stream);

        let task = tokio::spawn(async move {
            stream_session_updates(request_id, 1, &sessions_for_stream, &mut writer_half)
                .await
                .expect("session stream failed");
        });

        let snapshot = sessions_for_updates
            .start_session(
                "https://localhost".to_string(),
                Some("handoff-stream-1".to_string()),
            )
            .expect("start session should succeed");

        let (reader_half, _reader_writer) = reader_stream.into_split();
        let mut lines = BufReader::new(reader_half).lines();

        let first = lines
            .next_line()
            .await
            .expect("read first line failed")
            .expect("missing first line");

        let first_env: RpcEnvelope<DaemonResponse> =
            serde_json::from_str(&first).expect("parse first envelope failed");
        match first_env.payload {
            DaemonResponse::SessionUpdated {
                session_id,
                state,
                handoff_id,
                ..
            } => {
                assert_eq!(session_id, snapshot.session_id);
                assert_eq!(state, SessionState::PinEntry);
                assert_eq!(handoff_id.as_deref(), Some("handoff-stream-1"));
            }
            other => panic!("unexpected first payload: {other:?}"),
        }

        assert!(sessions_for_updates.cancel_session(snapshot.session_id));

        let second = lines
            .next_line()
            .await
            .expect("read second line failed")
            .expect("missing second line");
        let second_env: RpcEnvelope<DaemonResponse> =
            serde_json::from_str(&second).expect("parse second envelope failed");
        match second_env.payload {
            DaemonResponse::SessionCancelled { session_id } => {
                assert_eq!(session_id, snapshot.session_id);
            }
            other => panic!("unexpected second payload: {other:?}"),
        }

        drop(lines);
        task.abort();
    }

    #[tokio::test]
    async fn watch_sessions_emits_error_delta_when_session_fails() {
        let sessions = std::sync::Arc::new(SessionManager::new(StdDuration::from_secs(60)));
        let (reader_stream, writer_stream) = UnixStream::pair().expect("pair failed");
        let (_writer_reader, mut writer_half) = writer_stream.into_split();
        let request_id = uuid::Uuid::new_v4();

        let sessions_for_stream = std::sync::Arc::clone(&sessions);
        let sessions_for_updates = std::sync::Arc::clone(&sessions_for_stream);

        let task = tokio::spawn(async move {
            stream_session_updates(request_id, 1, &sessions_for_stream, &mut writer_half)
                .await
                .expect("session stream failed");
        });

        let snapshot = sessions_for_updates
            .start_session(
                "https://localhost".to_string(),
                Some("handoff-stream-2".to_string()),
            )
            .expect("start session should succeed");

        let (reader_half, _reader_writer) = reader_stream.into_split();
        let mut lines = BufReader::new(reader_half).lines();

        let first = lines
            .next_line()
            .await
            .expect("read first line failed")
            .expect("missing first line");

        let first_env: RpcEnvelope<DaemonResponse> =
            serde_json::from_str(&first).expect("parse first envelope failed");
        match first_env.payload {
            DaemonResponse::SessionUpdated {
                session_id,
                state,
                handoff_id,
                ..
            } => {
                assert_eq!(session_id, snapshot.session_id);
                assert_eq!(state, SessionState::PinEntry);
                assert_eq!(handoff_id.as_deref(), Some("handoff-stream-2"));
            }
            other => panic!("unexpected first payload: {other:?}"),
        }

        let failed = sessions_for_updates
            .fail_session(snapshot.session_id, "streamed failure".to_string())
            .expect("session should exist");
        assert_eq!(failed.state, SessionState::Error);

        let second = lines
            .next_line()
            .await
            .expect("read second line failed")
            .expect("missing second line");
        let second_env: RpcEnvelope<DaemonResponse> =
            serde_json::from_str(&second).expect("parse second envelope failed");
        match second_env.payload {
            DaemonResponse::SessionUpdated {
                session_id,
                state,
                error,
                handoff_id,
            } => {
                assert_eq!(session_id, snapshot.session_id);
                assert_eq!(state, SessionState::Error);
                assert_eq!(error.as_deref(), Some("streamed failure"));
                assert_eq!(handoff_id.as_deref(), Some("handoff-stream-2"));
            }
            other => panic!("unexpected second payload: {other:?}"),
        }

        drop(lines);
        task.abort();
    }

    #[test]
    fn should_log_pcsc_error_transition_only_on_change() {
        assert!(should_log_pcsc_error_transition(&None, &Some("x".to_string())));
        assert!(should_log_pcsc_error_transition(&Some("x".to_string()), &None));
        assert!(should_log_pcsc_error_transition(
            &Some("x".to_string()),
            &Some("y".to_string())
        ));
        assert!(!should_log_pcsc_error_transition(
            &Some("x".to_string()),
            &Some("x".to_string())
        ));
        assert!(!should_log_pcsc_error_transition(&None, &None));
    }

    #[test]
    fn repeated_identical_pcsc_errors_only_trigger_single_transition() {
        let samples = vec![
            None,
            Some("reader busy".to_string()),
            Some("reader busy".to_string()),
            Some("reader busy".to_string()),
            None,
            None,
            Some("reader busy".to_string()),
        ];

        let mut previous: Option<String> = None;
        let mut transitions = 0_u32;

        for sample in samples {
            if should_log_pcsc_error_transition(&previous, &sample) {
                transitions = transitions.saturating_add(1);
                previous = sample;
            }
        }

        assert_eq!(
            transitions, 3,
            "expected transitions: enter error, clear error, re-enter same error"
        );
    }
}
