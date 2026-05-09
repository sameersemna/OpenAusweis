#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use openausweis_ipc::{
    ClientRequest, DaemonResponse, RpcEnvelope, SessionState, IPC_PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::Path;
use std::path::PathBuf;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

const DAEMON_SOCKET_PATH: &str = "/tmp/openausweis-daemon.sock";
const DAEMON_STATUS_EVENT: &str = "daemon-status";
const DAEMON_SESSION_EVENT: &str = "daemon-session";
const DAEMON_STATUS_STREAM_INTERVAL_MS: u64 = 1000;
const DAEMON_SESSION_STREAM_INTERVAL_MS: u64 = 500;
const DAEMON_STATUS_RECONNECT_DELAY_MS: u64 = 1500;
const DAEMON_SESSION_RECONNECT_DELAY_MS: u64 = 1500;
const DEFAULT_ALLOWED_EXACT_ORIGINS: &[&str] = &["http://localhost", "https://localhost"];
const DEFAULT_ALLOWED_SUFFIXES: &[&str] = &[".bundid.de", ".bund.de"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopDaemonStatus {
    healthy: bool,
    pcsc_available: bool,
    active_session_count: u32,
    readers: Vec<DesktopReaderStatus>,
    diagnostics: Vec<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopReaderStatus {
    name: String,
    card_present: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopSessionUpdate {
    connected: bool,
    session_id: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OriginPolicyPayload {
    allowed_exact_origins: Vec<String>,
    allowed_suffixes: Vec<String>,
}

#[tauri::command]
async fn probe_daemon_status() -> std::result::Result<DesktopDaemonStatus, String> {
    get_daemon_status().await.map_err(|err| format!("{err}"))
}

#[tauri::command]
async fn start_test_session() -> std::result::Result<DesktopSessionUpdate, String> {
    let response = send_unary_daemon_request(ClientRequest::StartSession {
        relying_party: "https://localhost".to_string(),
    })
    .await
    .map_err(|err| format!("{err}"))?;

    match response {
        DaemonResponse::SessionStarted { session_id, state } => Ok(DesktopSessionUpdate {
            connected: true,
            session_id: Some(session_id.to_string()),
            state: Some(session_state_wire_value(state).to_string()),
            error: None,
        }),
        DaemonResponse::Error { code, message } => Err(format!("daemon error {code}: {message}")),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
async fn submit_session_pin(session_id: String, pin: String) -> std::result::Result<DesktopSessionUpdate, String> {
    let session_id = Uuid::parse_str(&session_id)
        .with_context(|| format!("invalid session id: {session_id}"))
        .map_err(|err| format!("{err}"))?;

    let response = send_unary_daemon_request(ClientRequest::SubmitPin { session_id, pin })
        .await
        .map_err(|err| format!("{err}"))?;

    match response {
        DaemonResponse::SessionUpdated {
            session_id,
            state,
            error,
        } => Ok(DesktopSessionUpdate {
            connected: true,
            session_id: Some(session_id.to_string()),
            state: Some(session_state_wire_value(state).to_string()),
            error,
        }),
        DaemonResponse::Error { code, message } => Err(format!("daemon error {code}: {message}")),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
async fn cancel_session(session_id: String) -> std::result::Result<(), String> {
    let session_id = Uuid::parse_str(&session_id)
        .with_context(|| format!("invalid session id: {session_id}"))
        .map_err(|err| format!("{err}"))?;

    let response = send_unary_daemon_request(ClientRequest::CancelSession { session_id })
        .await
        .map_err(|err| format!("{err}"))?;

    match response {
        DaemonResponse::SessionCancelled { .. } => Ok(()),
        DaemonResponse::Error { code, message } => {
            Err(format!("daemon error {code}: {message}"))
        }
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
async fn get_origin_policy() -> std::result::Result<OriginPolicyPayload, String> {
    read_origin_policy().await.map_err(|err| format!("{err}"))
}

#[tauri::command]
async fn save_origin_policy(policy: OriginPolicyPayload) -> std::result::Result<(), String> {
    validate_policy(&policy).map_err(|err| format!("{err}"))?;
    write_origin_policy(&policy)
        .await
        .map_err(|err| format!("{err}"))
}

fn default_origin_policy() -> OriginPolicyPayload {
    OriginPolicyPayload {
        allowed_exact_origins: DEFAULT_ALLOWED_EXACT_ORIGINS
            .iter()
            .map(|value| value.to_string())
            .collect(),
        allowed_suffixes: DEFAULT_ALLOWED_SUFFIXES
            .iter()
            .map(|value| value.to_string())
            .collect(),
    }
}

fn policy_file_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("OPENAUSWEIS_POLICY_DIR") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }

    if let Ok(path) = std::env::var("OPENAUSWEIS_POLICY_FILE") {
        if !path.trim().is_empty() {
            let file_path = PathBuf::from(path);
            if let Some(parent) = file_path.parent() {
                return Ok(parent.join(
                    file_path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("origin-policy"),
                ));
            }

            return Ok(PathBuf::from("origin-policy"));
        }
    }

    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("openausweis")
        .join("origin-policy"))
}

fn legacy_policy_file_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("OPENAUSWEIS_POLICY_FILE") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }

    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("openausweis")
        .join("origin-policy.json"))
}

fn policy_bundle_version_dir(root: &Path) -> PathBuf {
    root.join("versions").join(Uuid::new_v4().to_string())
}

fn policy_bundle_policy_path(bundle_dir: &Path) -> PathBuf {
    bundle_dir.join("policy.json")
}

fn policy_bundle_checksum_path(bundle_dir: &Path) -> PathBuf {
    bundle_dir.join("policy.sha256")
}

fn policy_bundle_current_dir(root: &Path) -> PathBuf {
    root.join("current")
}

async fn read_origin_policy() -> Result<OriginPolicyPayload> {
    let root = policy_file_path()?;
    if let Ok(policy) = read_policy_bundle(policy_bundle_current_dir(&root).as_path()).await {
        return Ok(policy);
    }

    let legacy_path = legacy_policy_file_path()?;
    match tokio::fs::read_to_string(&legacy_path).await {
        Ok(content) => {
            validate_policy_checksum_legacy(&legacy_path, content.as_bytes())?;
            let parsed: OriginPolicyPayload =
                serde_json::from_str(&content).context("policy file contains invalid JSON")?;
            validate_policy(&parsed)?;
            Ok(parsed)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(default_origin_policy()),
        Err(err) => {
            Err(err).with_context(|| format!("failed to read policy file at {legacy_path:?}"))
        }
    }
}

async fn write_origin_policy(policy: &OriginPolicyPayload) -> Result<()> {
    let root = policy_file_path()?;
    let parent = root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("policy bundle root has no parent directory"))?;
    tokio::fs::create_dir_all(parent)
        .await
        .with_context(|| format!("failed to create policy root parent at {parent:?}"))?;

    tokio::fs::create_dir_all(root.join("versions"))
        .await
        .with_context(|| format!("failed to create policy versions directory at {root:?}"))?;

    let encoded =
        serde_json::to_string_pretty(policy).context("failed to serialize policy for writing")?;
    write_policy_bundle(&root, encoded.as_bytes()).await?;

    Ok(())
}

async fn read_policy_bundle(bundle_dir: &Path) -> Result<OriginPolicyPayload> {
    let policy_path = policy_bundle_policy_path(bundle_dir);
    let checksum_path = policy_bundle_checksum_path(bundle_dir);

    let content = tokio::fs::read_to_string(&policy_path)
        .await
        .with_context(|| format!("failed to read policy file at {policy_path:?}"))?;
    validate_policy_checksum(&checksum_path, content.as_bytes())?;
    let parsed: OriginPolicyPayload =
        serde_json::from_str(&content).context("policy file contains invalid JSON")?;
    validate_policy(&parsed)?;
    Ok(parsed)
}

async fn write_policy_bundle(root: &Path, contents: &[u8]) -> Result<()> {
    let version_dir = policy_bundle_version_dir(root);
    tokio::fs::create_dir_all(&version_dir)
        .await
        .with_context(|| format!("failed to create policy version dir at {version_dir:?}"))?;

    let policy_path = policy_bundle_policy_path(&version_dir);
    let checksum_path = policy_bundle_checksum_path(&version_dir);
    let checksum = checksum_hex(contents);

    tokio::fs::write(&policy_path, contents)
        .await
        .with_context(|| format!("failed to write policy file at {policy_path:?}"))?;
    tokio::fs::write(&checksum_path, checksum)
        .await
        .with_context(|| format!("failed to write checksum file at {checksum_path:?}"))?;

    let current_link = policy_bundle_current_dir(root);
    let temp_link = root.join(format!("current.tmp.{}", Uuid::new_v4()));

    #[cfg(unix)]
    {
        let _ = tokio::fs::remove_file(&temp_link).await;
    }

    #[cfg(unix)]
    {
        symlink(&version_dir, &temp_link)
            .with_context(|| format!("failed to create policy bundle symlink at {temp_link:?}"))?;

        tokio::fs::rename(&temp_link, &current_link)
            .await
            .with_context(|| {
                format!("failed to update policy bundle symlink at {current_link:?}")
            })?;
    }

    Ok(())
}

fn validate_policy_checksum_legacy(path: &Path, contents: &[u8]) -> Result<()> {
    let checksum_path = policy_checksum_path(path);
    validate_policy_checksum(&checksum_path, contents)
}

fn validate_policy_checksum(checksum_path: &Path, contents: &[u8]) -> Result<()> {
    let stored = std::fs::read_to_string(&checksum_path)
        .with_context(|| format!("missing policy checksum at {checksum_path:?}"))?;
    let expected = checksum_hex(contents);
    let actual = stored.trim();

    if actual != expected {
        return Err(anyhow::anyhow!(
            "policy checksum mismatch at {checksum_path:?}: expected {expected}, got {actual}"
        ));
    }

    Ok(())
}

fn policy_checksum_path(path: &Path) -> PathBuf {
    path.with_extension("json.sha256")
}

fn checksum_hex(contents: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents);
    let digest = hasher.finalize();
    format!("{digest:x}")
}

fn validate_policy(policy: &OriginPolicyPayload) -> Result<()> {
    if policy.allowed_exact_origins.is_empty() {
        return Err(anyhow::anyhow!(
            "allowedExactOrigins must contain at least one origin"
        ));
    }

    if policy.allowed_suffixes.is_empty() {
        return Err(anyhow::anyhow!(
            "allowedSuffixes must contain at least one suffix"
        ));
    }

    for origin in &policy.allowed_exact_origins {
        let parsed = url::Url::parse(origin)
            .with_context(|| format!("invalid exact origin URL: {origin}"))?;

        if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(anyhow::anyhow!(
                "exact origin must not contain path/query/fragment: {origin}"
            ));
        }

        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(anyhow::anyhow!(
                "exact origin must use http or https scheme: {origin}"
            ));
        }
    }

    for suffix in &policy.allowed_suffixes {
        if !suffix.starts_with('.') || suffix.len() < 3 {
            return Err(anyhow::anyhow!(
                "domain suffix must start with '.' and be at least 3 chars: {suffix}"
            ));
        }
    }

    Ok(())
}

async fn get_daemon_status() -> Result<DesktopDaemonStatus> {
    match send_unary_daemon_request(ClientRequest::GetStatus).await? {
        DaemonResponse::Status(status) => Ok(to_desktop_status(status)),
        DaemonResponse::Error { code, message } => {
            Err(anyhow::anyhow!("daemon error {code}: {message}"))
        }
        other => Err(anyhow::anyhow!("unexpected daemon response: {other:?}")),
    }
}

async fn send_unary_daemon_request(request_payload: ClientRequest) -> Result<DaemonResponse> {
    let stream = UnixStream::connect(DAEMON_SOCKET_PATH)
        .await
        .with_context(|| format!("failed to connect to daemon socket at {DAEMON_SOCKET_PATH}"))?;

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let request_id = Uuid::new_v4();
    let request = RpcEnvelope::new(request_id, request_payload);

    let payload = serde_json::to_string(&request).context("failed to serialize daemon request")?;
    writer
        .write_all(payload.as_bytes())
        .await
        .context("failed to write daemon request")?;
    writer
        .write_all(b"\n")
        .await
        .context("failed to write daemon request newline")?;

    let line = lines
        .next_line()
        .await
        .context("failed to read daemon response")?
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection before responding"))?;

    let response: RpcEnvelope<DaemonResponse> =
        serde_json::from_str(&line).context("failed to parse daemon response")?;

    validate_response_metadata(request_id, &response)?;

    Ok(response.payload)
}

fn to_desktop_status(status: openausweis_ipc::DaemonStatus) -> DesktopDaemonStatus {
    DesktopDaemonStatus {
        healthy: status.healthy,
        pcsc_available: status.pcsc_available,
        active_session_count: status.active_session_count,
        readers: status
            .readers
            .into_iter()
            .map(|reader| DesktopReaderStatus {
                name: reader.name,
                card_present: reader.card_present,
                error: reader.error,
            })
            .collect(),
        diagnostics: status.diagnostics,
        last_error: status.last_error,
    }
}

fn disconnected_status(details: String) -> DesktopDaemonStatus {
    DesktopDaemonStatus {
        healthy: false,
        pcsc_available: false,
        active_session_count: 0,
        readers: Vec::new(),
        diagnostics: vec![details],
        last_error: Some("daemon stream disconnected".to_string()),
    }
}

fn spawn_daemon_status_stream(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(err) = stream_daemon_status_once(&app).await {
                let _ = app.emit(
                    DAEMON_STATUS_EVENT,
                    disconnected_status(format!("daemon status stream error: {err}")),
                );
                sleep(reconnect_backoff_delay()).await;
            }
        }
    });
}

fn spawn_daemon_session_stream(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(err) = stream_daemon_sessions_once(&app).await {
                let _ = app.emit(
                    DAEMON_SESSION_EVENT,
                    disconnected_session_update(format!("daemon session stream error: {err}")),
                );
                sleep(reconnect_session_backoff_delay()).await;
            }
        }
    });
}

fn reconnect_backoff_delay() -> Duration {
    Duration::from_millis(DAEMON_STATUS_RECONNECT_DELAY_MS)
}

fn reconnect_session_backoff_delay() -> Duration {
    Duration::from_millis(DAEMON_SESSION_RECONNECT_DELAY_MS)
}

async fn stream_daemon_status_once(app: &AppHandle) -> Result<()> {
    let stream = UnixStream::connect(DAEMON_SOCKET_PATH)
        .await
        .with_context(|| format!("failed to connect to daemon socket at {DAEMON_SOCKET_PATH}"))?;

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let request_id = Uuid::new_v4();
    let request = RpcEnvelope::new(
        request_id,
        ClientRequest::WatchStatus {
            interval_ms: DAEMON_STATUS_STREAM_INTERVAL_MS,
        },
    );

    let payload = serde_json::to_string(&request).context("failed to serialize daemon request")?;
    writer
        .write_all(payload.as_bytes())
        .await
        .context("failed to write daemon request")?;
    writer
        .write_all(b"\n")
        .await
        .context("failed to write daemon request newline")?;

    while let Some(line) = lines
        .next_line()
        .await
        .context("failed to read daemon stream response")?
    {
        let status = parse_stream_response_line(request_id, &line)?;
        app.emit(DAEMON_STATUS_EVENT, status)
            .context("failed to emit daemon status event")?;
    }

    Err(anyhow::anyhow!("daemon stream closed connection"))
}

async fn stream_daemon_sessions_once(app: &AppHandle) -> Result<()> {
    let stream = UnixStream::connect(DAEMON_SOCKET_PATH)
        .await
        .with_context(|| format!("failed to connect to daemon socket at {DAEMON_SOCKET_PATH}"))?;

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let request_id = Uuid::new_v4();
    let request = RpcEnvelope::new(
        request_id,
        ClientRequest::WatchSessions {
            interval_ms: DAEMON_SESSION_STREAM_INTERVAL_MS,
        },
    );

    let payload = serde_json::to_string(&request).context("failed to serialize daemon request")?;
    writer
        .write_all(payload.as_bytes())
        .await
        .context("failed to write daemon request")?;
    writer
        .write_all(b"\n")
        .await
        .context("failed to write daemon request newline")?;

    while let Some(line) = lines
        .next_line()
        .await
        .context("failed to read daemon session stream response")?
    {
        let update = parse_session_stream_response_line(request_id, &line)?;
        app.emit(DAEMON_SESSION_EVENT, update)
            .context("failed to emit daemon session event")?;
    }

    Err(anyhow::anyhow!("daemon session stream closed connection"))
}

fn parse_stream_response_line(
    expected_request_id: Uuid,
    line: &str,
) -> Result<DesktopDaemonStatus> {
    let response: RpcEnvelope<DaemonResponse> =
        serde_json::from_str(line).context("failed to parse daemon stream response")?;

    validate_response_metadata(expected_request_id, &response)?;

    match response.payload {
        DaemonResponse::Status(status) => Ok(to_desktop_status(status)),
        DaemonResponse::Error { code, message } => {
            Err(anyhow::anyhow!("daemon stream error {code}: {message}"))
        }
        other => Err(anyhow::anyhow!(
            "unexpected daemon stream response: {other:?}"
        )),
    }
}

fn parse_session_stream_response_line(
    expected_request_id: Uuid,
    line: &str,
) -> Result<DesktopSessionUpdate> {
    let response: RpcEnvelope<DaemonResponse> =
        serde_json::from_str(line).context("failed to parse daemon session stream response")?;

    validate_response_metadata(expected_request_id, &response)?;

    match response.payload {
        DaemonResponse::SessionUpdated {
            session_id,
            state,
            error,
        } => Ok(DesktopSessionUpdate {
            connected: true,
            session_id: Some(session_id.to_string()),
            state: Some(session_state_wire_value(state).to_string()),
            error,
        }),
        DaemonResponse::SessionCancelled { .. } => Ok(DesktopSessionUpdate {
            connected: true,
            session_id: None,
            state: Some("IDLE".to_string()),
            error: None,
        }),
        DaemonResponse::Error { code, message } => {
            Err(anyhow::anyhow!("daemon session stream error {code}: {message}"))
        }
        other => Err(anyhow::anyhow!(
            "unexpected daemon session stream response: {other:?}"
        )),
    }
}

fn session_state_wire_value(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle => "IDLE",
        SessionState::Active => "ACTIVE",
        SessionState::PinEntry => "PIN_ENTRY",
        SessionState::CardInteraction => "CARD_INTERACTION",
        SessionState::Completed => "COMPLETED",
        SessionState::Error => "ERROR",
    }
}

fn disconnected_session_update(details: String) -> DesktopSessionUpdate {
    DesktopSessionUpdate {
        connected: false,
        session_id: None,
        state: Some("IDLE".to_string()),
        error: Some(details),
    }
}

fn validate_response_metadata(
    expected_request_id: Uuid,
    response: &RpcEnvelope<DaemonResponse>,
) -> Result<()> {
    if response.protocol_version != IPC_PROTOCOL_VERSION {
        return Err(anyhow::anyhow!(
            "daemon protocol mismatch: expected {}, got {}",
            IPC_PROTOCOL_VERSION,
            response.protocol_version
        ));
    }

    if response.request_id != expected_request_id {
        return Err(anyhow::anyhow!(
            "daemon returned mismatched request id: expected {}, got {}",
            expected_request_id,
            response.request_id
        ));
    }

    Ok(())
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let tray_menu = Menu::with_items(app, &[&quit_item])?;
    let quit_id = quit_item.id().clone();

    TrayIconBuilder::with_id("main")
        .menu(&tray_menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app_handle, event| {
            if event.id() == &quit_id {
                app_handle.exit(0);
            }
        })
        .build(app)?;

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            setup_tray(app)?;
            spawn_daemon_status_stream(app.handle().clone());
            spawn_daemon_session_stream(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            probe_daemon_status,
            start_test_session,
            submit_session_pin,
            cancel_session,
            get_origin_policy,
            save_origin_policy
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri app");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    fn sample_response(request_id: Uuid, protocol_version: u16) -> RpcEnvelope<DaemonResponse> {
        RpcEnvelope {
            protocol_version,
            request_id,
            payload: DaemonResponse::Error {
                code: "TEST".to_string(),
                message: "test".to_string(),
            },
        }
    }

    #[test]
    fn validate_response_metadata_accepts_matching_values() {
        let request_id = Uuid::new_v4();
        let response = sample_response(request_id, IPC_PROTOCOL_VERSION);
        assert!(validate_response_metadata(request_id, &response).is_ok());
    }

    #[test]
    fn validate_response_metadata_rejects_protocol_mismatch() {
        let request_id = Uuid::new_v4();
        let response = sample_response(request_id, IPC_PROTOCOL_VERSION + 1);
        let error = validate_response_metadata(request_id, &response)
            .expect_err("expected protocol mismatch error");
        assert!(error.to_string().contains("protocol mismatch"));
    }

    #[test]
    fn validate_response_metadata_rejects_request_id_mismatch() {
        let expected_request_id = Uuid::new_v4();
        let response = sample_response(Uuid::new_v4(), IPC_PROTOCOL_VERSION);
        let error = validate_response_metadata(expected_request_id, &response)
            .expect_err("expected request id mismatch error");
        assert!(error.to_string().contains("mismatched request id"));
    }

    #[test]
    fn disconnected_status_contains_diagnostic_details() {
        let status = disconnected_status("stream failed".to_string());
        assert!(!status.healthy);
        assert!(!status.pcsc_available);
        assert_eq!(status.diagnostics, vec!["stream failed".to_string()]);
        assert_eq!(
            status.last_error.as_deref(),
            Some("daemon stream disconnected")
        );
    }

    #[test]
    fn reconnect_backoff_delay_is_at_least_one_second() {
        assert!(reconnect_backoff_delay() >= Duration::from_millis(1000));
    }

    #[test]
    fn reconnect_backoff_delay_matches_configured_value() {
        assert_eq!(
            reconnect_backoff_delay(),
            Duration::from_millis(DAEMON_STATUS_RECONNECT_DELAY_MS)
        );
    }

    #[tokio::test]
    async fn parse_stream_response_line_rejects_protocol_mismatch_from_socket_frame() {
        let request_id = Uuid::new_v4();
        let envelope = RpcEnvelope {
            protocol_version: IPC_PROTOCOL_VERSION + 1,
            request_id,
            payload: DaemonResponse::Error {
                code: "TEST".to_string(),
                message: "protocol mismatch simulation".to_string(),
            },
        };

        let (client, mut server) = UnixStream::pair().expect("pair failed");
        let payload = serde_json::to_string(&envelope).expect("serialize failed");
        server
            .write_all(payload.as_bytes())
            .await
            .expect("write failed");
        server.write_all(b"\n").await.expect("write newline failed");
        drop(server);

        let (reader, _) = client.into_split();
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await
            .expect("read line failed")
            .expect("missing line");

        let error = parse_stream_response_line(request_id, &line)
            .expect_err("expected protocol mismatch parse failure");
        assert!(error.to_string().contains("protocol mismatch"));
    }

    #[test]
    fn fallback_status_includes_stream_error_context() {
        let err = anyhow::anyhow!("daemon protocol mismatch: expected 1, got 2");
        let status = disconnected_status(format!("daemon status stream error: {err}"));
        assert!(status.diagnostics[0].contains("daemon status stream error"));
        assert!(status.diagnostics[0].contains("protocol mismatch"));
    }
}
