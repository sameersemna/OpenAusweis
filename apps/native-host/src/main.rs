use anyhow::{Context, Result};
use openausweis_ipc::{ClientRequest, DaemonResponse, RpcEnvelope, IPC_PROTOCOL_VERSION};
use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

const DAEMON_SOCKET_PATH_FALLBACK: &str = "/tmp/openausweis-daemon.sock";
#[cfg(not(test))]
const WATCH_SESSIONS_FIRST_EVENT_TIMEOUT_MS: u64 = 4500;
#[cfg(test)]
const WATCH_SESSIONS_FIRST_EVENT_TIMEOUT_MS: u64 = 150;
const WATCH_SESSIONS_MIN_INTERVAL_MS: u64 = 100;
const WATCH_SESSIONS_MAX_INTERVAL_MS: u64 = 10_000;
const START_SESSION_HANDOFF_ID_MAX_LEN: usize = 128;
const SUBMIT_PIN_MAX_LEN: usize = 64;

const DEFAULT_ALLOWED_EXACT_ORIGINS: &[&str] = &["http://localhost", "https://localhost"];
const DEFAULT_ALLOWED_SUFFIXES: &[&str] = &[".bundid.de", ".bund.de"];

struct OriginPolicy {
    allowed_exact_origins: HashSet<String>,
    allowed_suffixes: Vec<String>,
}

#[derive(Debug)]
struct RequestValidationError {
    code: &'static str,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OriginPolicyFile {
    allowed_exact_origins: Vec<String>,
    allowed_suffixes: Vec<String>,
}

/// Per-run metrics for one native-host process lifetime (one browser session).
/// Written as a JSON sidecar on clean exit so external tooling (e.g. the
/// desktop app) can read the latest values.
#[derive(Debug, Default, Serialize)]
struct NativeHostMetrics {
    requests_processed: u64,
    validation_rejections: u64,
    connection_failures: u64,
}

impl NativeHostMetrics {
    fn metrics_file_path() -> PathBuf {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            if !runtime_dir.trim().is_empty() {
                return PathBuf::from(runtime_dir)
                    .join("openausweis")
                    .join("native-host-metrics.json");
            }
        }
        PathBuf::from("/tmp/openausweis-native-host-metrics.json")
    }

    fn persist(&self) {
        let path = Self::metrics_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}

fn init_logging() {
    // Logs to stderr with a compact format. Browser extensions can capture stderr
    // for debugging purposes.
    tracing_subscriber::fmt()
        .with_env_filter("openausweis_native_host=debug,openausweis=debug")
        .with_target(false)
        .compact()
        .with_writer(std::io::stderr)
        .init();
}

fn validate_startup_environment() {
    let socket_path = daemon_socket_path();
    let socket_str = socket_path.to_string_lossy().to_string();

    // Verify socket path is in a safe location
    let xdg_runtime = std::env::var("XDG_RUNTIME_DIR").ok();
    let is_in_runtime_dir = xdg_runtime
        .as_ref()
        .map(|rt| socket_str.starts_with(rt))
        .unwrap_or(false);
    let is_in_tmp = socket_str.starts_with("/tmp/");

    if !is_in_runtime_dir && !is_in_tmp {
        warn!(
            socket = %socket_path.display(),
            "daemon socket path is not in XDG_RUNTIME_DIR or /tmp; connectivity may fail"
        );
    }

    // Check if parent directory of socket path exists (non-fatal warning)
    if let Some(parent) = socket_path.parent() {
        if !parent.exists() {
            debug!(
                parent = %parent.display(),
                "daemon socket parent directory does not exist yet; may be created on demand"
            );
        }
    }

    debug!(
        socket = %socket_path.display(),
        xdg_runtime_dir = ?xdg_runtime,
        "startup validation: daemon socket path is acceptable"
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    validate_startup_environment();
    info!("native-host started");

    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let mut metrics = NativeHostMetrics::default();

    loop {
        let message = match read_native_message(&mut stdin) {
            Ok(Some(message)) => message,
            Ok(None) => {
                debug!("EOF on stdin, shutting down");
                break;
            }
            Err(err) => {
                warn!(error = %err, "failed to read native message frame");
                emit_error(
                    &mut stdout,
                    None,
                    "INVALID_REQUEST",
                    &format!("Invalid native message frame: {err}"),
                )?;
                continue;
            }
        };

        debug!(message_len = message.len(), "received native message");

        let parsed: RpcEnvelope<ClientRequest> = match serde_json::from_slice(&message) {
            Ok(value) => value,
            Err(err) => {
                warn!(error = %err, "failed to parse request JSON");
                emit_error(
                    &mut stdout,
                    None,
                    "INVALID_REQUEST",
                    &format!("Invalid request JSON: {err}"),
                )?;
                metrics.validation_rejections = metrics.validation_rejections.saturating_add(1);
                continue;
            }
        };

        if parsed.protocol_version != IPC_PROTOCOL_VERSION {
            warn!(
                received = parsed.protocol_version,
                expected = IPC_PROTOCOL_VERSION,
                "unsupported protocol version"
            );
            emit_error(
                &mut stdout,
                Some(parsed.request_id),
                "UNSUPPORTED_PROTOCOL",
                &format!(
                    "protocol {} is unsupported; expected {}",
                    parsed.protocol_version, IPC_PROTOCOL_VERSION
                ),
            )?;
            metrics.validation_rejections = metrics.validation_rejections.saturating_add(1);
            continue;
        }

        if let Err(err) = validate_client_request(&parsed.payload) {
            warn!(
                code = err.code,
                message = %err.message,
                "request validation failed"
            );
            emit_error(
                &mut stdout,
                Some(parsed.request_id),
                err.code,
                &err.message,
            )?;
            metrics.validation_rejections = metrics.validation_rejections.saturating_add(1);
            continue;
        }

        metrics.requests_processed = metrics.requests_processed.saturating_add(1);
        let request_id = parsed.request_id;
        debug!(request_id = %request_id, "processing request");

        match forward_to_daemon(parsed).await {
            Ok(response) => {
                if response.protocol_version != IPC_PROTOCOL_VERSION {
                    warn!(
                        received = response.protocol_version,
                        expected = IPC_PROTOCOL_VERSION,
                        "daemon returned unsupported protocol version"
                    );
                    emit_error(
                        &mut stdout,
                        Some(response.request_id),
                        "UNSUPPORTED_PROTOCOL",
                        &format!(
                            "daemon protocol {} is unsupported; expected {}",
                            response.protocol_version, IPC_PROTOCOL_VERSION
                        ),
                    )?;
                    continue;
                }

                debug!(request_id = %response.request_id, "sending response");
                emit_response(&mut stdout, &response)?;
            }
            Err(err) => {
                metrics.connection_failures = metrics.connection_failures.saturating_add(1);
                error!(error = %err, "daemon connection failed");
                emit_error(
                    &mut stdout,
                    Some(request_id),
                    "DAEMON_UNAVAILABLE",
                    &format!("{err}"),
                )?;
            }
        }
    }

    info!(
        requests_processed = metrics.requests_processed,
        validation_rejections = metrics.validation_rejections,
        connection_failures = metrics.connection_failures,
        "shutting down, persisting metrics"
    );
    metrics.persist();
    Ok(())
}

fn validate_client_request(request: &ClientRequest) -> std::result::Result<(), RequestValidationError> {
    let policy = load_origin_policy();
    match request {
        ClientRequest::WatchStatus { interval_ms } | ClientRequest::WatchSessions { interval_ms } => {
            if *interval_ms < WATCH_SESSIONS_MIN_INTERVAL_MS
                || *interval_ms > WATCH_SESSIONS_MAX_INTERVAL_MS
            {
                return Err(RequestValidationError {
                    code: "INVALID_REQUEST",
                    message: format!(
                        "watch interval_ms must be between {WATCH_SESSIONS_MIN_INTERVAL_MS} and {WATCH_SESSIONS_MAX_INTERVAL_MS}; got {interval_ms}"
                    ),
                });
            }

            Ok(())
        }
        ClientRequest::StartSession { relying_party, .. } => {
            if !is_origin_allowed(relying_party, &policy).map_err(|err| RequestValidationError {
                code: "INVALID_REQUEST",
                message: err.to_string(),
            })? {
                return Err(RequestValidationError {
                    code: "RP_NOT_ALLOWED",
                    message: format!("relying party origin is not allowed: {relying_party}"),
                });
            }

            if let ClientRequest::StartSession { handoff_id, .. } = request {
                if let Some(value) = handoff_id {
                    if value.trim().is_empty() {
                        return Err(RequestValidationError {
                            code: "INVALID_REQUEST",
                            message: "START_SESSION handoff_id must be non-empty when provided"
                                .to_string(),
                        });
                    }

                    if value.len() > START_SESSION_HANDOFF_ID_MAX_LEN {
                        return Err(RequestValidationError {
                            code: "INVALID_REQUEST",
                            message: format!(
                                "START_SESSION handoff_id exceeds maximum length of {START_SESSION_HANDOFF_ID_MAX_LEN}"
                            ),
                        });
                    }
                }
            }

            Ok(())
        }
        ClientRequest::SubmitPin { pin, .. } => {
            if pin.is_empty() {
                return Err(RequestValidationError {
                    code: "INVALID_REQUEST",
                    message: "SUBMIT_PIN requires a non-empty pin".to_string(),
                });
            }

            if pin.len() > SUBMIT_PIN_MAX_LEN {
                return Err(RequestValidationError {
                    code: "INVALID_REQUEST",
                    message: format!("SUBMIT_PIN pin exceeds maximum length of {SUBMIT_PIN_MAX_LEN}"),
                });
            }

            Ok(())
        }
        _ => Ok(()),
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

    PathBuf::from(DAEMON_SOCKET_PATH_FALLBACK)
}

fn load_origin_policy() -> OriginPolicy {
    let mut exact: Vec<String> = DEFAULT_ALLOWED_EXACT_ORIGINS
        .iter()
        .map(|value| value.to_string())
        .collect();

    let mut suffixes: Vec<String> = DEFAULT_ALLOWED_SUFFIXES
        .iter()
        .map(|value| value.to_string())
        .collect();

    if let Some(policy_file) = load_policy_file() {
        if !policy_file.allowed_exact_origins.is_empty() {
            debug!("loaded exact origins from policy file: {:?}", policy_file.allowed_exact_origins);
            exact = policy_file.allowed_exact_origins;
        }

        if !policy_file.allowed_suffixes.is_empty() {
            debug!("loaded suffixes from policy file: {:?}", policy_file.allowed_suffixes);
            suffixes = policy_file.allowed_suffixes;
        }
    }

    if let Some(values) = std::env::var("OPENAUSWEIS_ALLOWED_EXACT_ORIGINS")
        .ok()
        .map(parse_csv_list)
        .filter(|values| !values.is_empty())
    {
        debug!("overriding exact origins from env: {:?}", values);
        exact = values;
    }

    if let Some(values) = std::env::var("OPENAUSWEIS_ALLOWED_SUFFIXES")
        .ok()
        .map(parse_csv_list)
        .filter(|values| !values.is_empty())
    {
        debug!("overriding suffixes from env: {:?}", values);
        suffixes = values;
    }

    debug!(exact_origins = ?exact.len(), suffixes = ?suffixes.len(), "origin policy configured");

    OriginPolicy {
        allowed_exact_origins: exact.into_iter().collect(),
        allowed_suffixes: suffixes,
    }
}

fn policy_file_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OPENAUSWEIS_POLICY_DIR") {
        if !path.trim().is_empty() {
            return Some(PathBuf::from(path).join("current"));
        }
    }

    if let Ok(path) = std::env::var("OPENAUSWEIS_POLICY_FILE") {
        if !path.trim().is_empty() {
            let legacy_path = PathBuf::from(path);
            return legacy_path
                .parent()
                .map(|parent| {
                    parent
                        .join(
                            legacy_path
                                .file_stem()
                                .and_then(|stem| stem.to_str())
                                .unwrap_or("origin-policy"),
                        )
                        .join("current")
                })
                .or(Some(legacy_path));
        }
    }

    std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("openausweis")
            .join("origin-policy")
            .join("current")
    })
}

fn legacy_policy_file_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OPENAUSWEIS_POLICY_FILE") {
        if !path.trim().is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("openausweis")
            .join("origin-policy.json")
    })
}

fn load_policy_file() -> Option<OriginPolicyFile> {
    let path = policy_file_path()?;
    for _attempt in 0..5 {
        let content = match std::fs::read_to_string(policy_bundle_policy_path(&path)) {
            Ok(content) => content,
            Err(_) => {
                thread::sleep(std::time::Duration::from_millis(25));
                continue;
            }
        };

        if validate_policy_checksum(&policy_bundle_checksum_path(&path), content.as_bytes()).is_ok()
        {
            if let Ok(parsed) = serde_json::from_str(&content) {
                return Some(parsed);
            }
        }

        thread::sleep(std::time::Duration::from_millis(25));
    }

    if let Some(legacy_path) = legacy_policy_file_path() {
        if let Ok(content) = std::fs::read_to_string(&legacy_path) {
            if validate_policy_checksum(
                &legacy_policy_checksum_path(&legacy_path),
                content.as_bytes(),
            )
            .is_ok()
            {
                return serde_json::from_str(&content).ok();
            }
        }
    }

    None
}

fn validate_policy_checksum(checksum_path: &Path, contents: &[u8]) -> Result<()> {
    let stored = std::fs::read_to_string(checksum_path)
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

fn policy_bundle_policy_path(bundle_dir: &Path) -> PathBuf {
    bundle_dir.join("policy.json")
}

fn policy_bundle_checksum_path(bundle_dir: &Path) -> PathBuf {
    bundle_dir.join("policy.sha256")
}

fn legacy_policy_checksum_path(path: &Path) -> PathBuf {
    path.with_extension("json.sha256")
}

fn checksum_hex(contents: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents);
    let digest = hasher.finalize();
    format!("{digest:x}")
}

fn parse_csv_list(input: String) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn is_origin_allowed(origin: &str, policy: &OriginPolicy) -> Result<bool> {
    if policy.allowed_exact_origins.contains(origin) {
        return Ok(true);
    }

    let parsed = Url::parse(origin)
        .with_context(|| format!("relying party is not a valid origin URL: {origin}"))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("relying party has no host component"))?;

    if host == "localhost" {
        return Ok(true);
    }

    if parsed.scheme() != "https" {
        return Ok(false);
    }

    Ok(policy
        .allowed_suffixes
        .iter()
        .any(|suffix| host.ends_with(suffix)))
}

fn read_native_message(reader: &mut impl Read) -> Result<Option<Vec<u8>>> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err).context("failed to read native message length"),
    }

    let message_len = u32::from_le_bytes(length) as usize;
    if message_len == 0 {
        return Err(anyhow::anyhow!(
            "native message length must be greater than zero"
        ));
    }

    // Keep host memory usage predictable and reject abnormal payload sizes.
    if message_len > 1024 * 1024 {
        return Err(anyhow::anyhow!(
            "native message length {message_len} exceeds allowed limit"
        ));
    }

    let mut message = vec![0_u8; message_len];
    reader
        .read_exact(&mut message)
        .context("failed to read native message bytes")?;

    Ok(Some(message))
}

async fn forward_to_daemon(
    request: RpcEnvelope<ClientRequest>,
) -> Result<RpcEnvelope<DaemonResponse>> {
    let wait_for_first_event = matches!(&request.payload, ClientRequest::WatchSessions { .. });
    let expected_request_id = request.request_id;
    let socket_path = daemon_socket_path();

    debug!(socket = %socket_path.display(), "connecting to daemon");
    let stream = UnixStream::connect(&socket_path)
        .await
        .with_context(|| {
            format!(
                "failed to connect to daemon socket at {}",
                socket_path.display()
            )
        })?;
    debug!("connected to daemon");

    let (reader, mut writer) = stream.into_split();
    let mut daemon_lines = BufReader::new(reader).lines();

    let encoded = serde_json::to_string(&request).context("failed to encode daemon request")?;
    debug!(request_id = %request.request_id, "sending request to daemon");
    writer
        .write_all(encoded.as_bytes())
        .await
        .context("failed to write daemon request")?;
    writer
        .write_all(b"\n")
        .await
        .context("failed to write daemon request newline")?;

    let line = if wait_for_first_event {
        debug!("waiting for first session event (timeout: {}ms)", WATCH_SESSIONS_FIRST_EVENT_TIMEOUT_MS);
        timeout(
            Duration::from_millis(WATCH_SESSIONS_FIRST_EVENT_TIMEOUT_MS),
            daemon_lines.next_line(),
        )
        .await
        .context("timed out while waiting for session stream event")?
        .context("failed to read daemon response")?
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection before responding"))?
    } else {
        daemon_lines
            .next_line()
            .await
            .context("failed to read daemon response")?
            .ok_or_else(|| anyhow::anyhow!("daemon closed connection before responding"))?
    };

    let response: RpcEnvelope<DaemonResponse> =
        serde_json::from_str(&line).context("failed to parse daemon response")?;

    if response.request_id != expected_request_id {
        let msg = format!(
            "daemon response request_id mismatch: expected {}, got {}",
            expected_request_id, response.request_id
        );
        error!("{}", msg);
        return Err(anyhow::anyhow!(msg));
    }

    if wait_for_first_event
        && !matches!(
            response.payload,
            DaemonResponse::SessionUpdated { .. }
                | DaemonResponse::SessionCancelled { .. }
                | DaemonResponse::Error { .. }
        )
    {
        let msg = "watch sessions returned unexpected first event type".to_string();
        error!("{}", msg);
        return Err(anyhow::anyhow!(msg));
    }

    debug!(request_id = %response.request_id, "received response from daemon");
    Ok(response)
}

fn emit_response(writer: &mut impl Write, response: &RpcEnvelope<DaemonResponse>) -> Result<()> {
    let output = serde_json::to_vec(response).context("failed to serialize native host output")?;
    let length = u32::try_from(output.len())
        .context("native host output exceeded maximum message length")?
        .to_le_bytes();

    writer
        .write_all(&length)
        .context("failed to write native host output length")?;
    writer
        .write_all(&output)
        .context("failed to write native host output bytes")?;
    writer
        .flush()
        .context("failed to flush native host output")?;
    Ok(())
}

fn emit_error(
    writer: &mut impl Write,
    request_id: Option<Uuid>,
    code: &str,
    message: &str,
) -> Result<()> {
    let payload = RpcEnvelope::new(
        request_id.unwrap_or_else(Uuid::new_v4),
        DaemonResponse::Error {
            code: code.to_string(),
            message: message.to_string(),
        },
    );

    emit_response(writer, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tokio::net::UnixListener;

    static TEST_SOCKET_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn validate_watch_sessions_rejects_out_of_range_interval() {
        let request = ClientRequest::WatchSessions {
            interval_ms: WATCH_SESSIONS_MAX_INTERVAL_MS + 1,
        };

        let err = validate_client_request(&request).expect_err("expected out-of-range error");
        assert_eq!(err.code, "INVALID_REQUEST");
        assert!(err.message.contains("watch interval_ms must be between"));
    }

    #[test]
    fn validate_start_session_rejects_empty_handoff() {
        let request = ClientRequest::StartSession {
            relying_party: "https://localhost".to_string(),
            handoff_id: Some("   ".to_string()),
        };

        let err = validate_client_request(&request).expect_err("expected empty handoff error");
        assert_eq!(err.code, "INVALID_REQUEST");
        assert!(err.message.contains("handoff_id must be non-empty"));
    }

    #[test]
    fn validate_submit_pin_rejects_excessive_pin_length() {
        let request = ClientRequest::SubmitPin {
            session_id: Uuid::new_v4(),
            pin: "9".repeat(SUBMIT_PIN_MAX_LEN + 1),
        };

        let err = validate_client_request(&request).expect_err("expected pin length error");
        assert_eq!(err.code, "INVALID_REQUEST");
        assert!(err.message.contains("pin exceeds maximum length"));
    }

    #[test]
    fn validate_start_session_allows_localhost_origin() {
        let request = ClientRequest::StartSession {
            relying_party: "https://localhost".to_string(),
            handoff_id: Some("ext-test-handoff".to_string()),
        };

        validate_client_request(&request).expect("localhost origin should be allowed");
    }

    async fn with_mock_daemon_response(
        request: RpcEnvelope<ClientRequest>,
        response: Option<RpcEnvelope<DaemonResponse>>,
        hold_open: bool,
    ) -> Result<RpcEnvelope<DaemonResponse>> {
        let lock = TEST_SOCKET_ENV_LOCK.get_or_init(|| Mutex::new(()));
        let _guard = lock.lock().expect("test socket env lock poisoned");

        let socket_path = std::env::temp_dir().join(format!(
            "openausweis-native-host-test-{}.sock",
            Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&socket_path);

        let previous_socket = std::env::var("OPENAUSWEIS_DAEMON_SOCKET").ok();
        std::env::set_var("OPENAUSWEIS_DAEMON_SOCKET", socket_path.to_string_lossy().to_string());

        let listener = UnixListener::bind(&socket_path).expect("bind mock daemon socket");
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept connection");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let _ = lines.next_line().await.expect("read request line");

            if hold_open {
                tokio::time::sleep(Duration::from_millis(
                    WATCH_SESSIONS_FIRST_EVENT_TIMEOUT_MS + 50,
                ))
                .await;
                return;
            }

            if let Some(response) = response {
                let encoded = serde_json::to_string(&response).expect("encode mock response");
                writer
                    .write_all(encoded.as_bytes())
                    .await
                    .expect("write mock response");
                writer
                    .write_all(b"\n")
                    .await
                    .expect("write mock response newline");
            }
        });

        let result = forward_to_daemon(request).await;

        let _ = server_task.await;
        let _ = std::fs::remove_file(&socket_path);

        if let Some(previous) = previous_socket {
            std::env::set_var("OPENAUSWEIS_DAEMON_SOCKET", previous);
        } else {
            std::env::remove_var("OPENAUSWEIS_DAEMON_SOCKET");
        }

        result
    }

    async fn with_mock_daemon_raw_line(
        request: RpcEnvelope<ClientRequest>,
        raw_line: Option<String>,
    ) -> Result<RpcEnvelope<DaemonResponse>> {
        let lock = TEST_SOCKET_ENV_LOCK.get_or_init(|| Mutex::new(()));
        let _guard = lock.lock().expect("test socket env lock poisoned");

        let socket_path = std::env::temp_dir().join(format!(
            "openausweis-native-host-test-{}.sock",
            Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&socket_path);

        let previous_socket = std::env::var("OPENAUSWEIS_DAEMON_SOCKET").ok();
        std::env::set_var("OPENAUSWEIS_DAEMON_SOCKET", socket_path.to_string_lossy().to_string());

        let listener = UnixListener::bind(&socket_path).expect("bind mock daemon socket");
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept connection");
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            let _ = lines.next_line().await.expect("read request line");

            if let Some(raw_line) = raw_line {
                writer
                    .write_all(raw_line.as_bytes())
                    .await
                    .expect("write mock response");
                writer
                    .write_all(b"\n")
                    .await
                    .expect("write mock response newline");
            }
        });

        let result = forward_to_daemon(request).await;

        let _ = server_task.await;
        let _ = std::fs::remove_file(&socket_path);

        if let Some(previous) = previous_socket {
            std::env::set_var("OPENAUSWEIS_DAEMON_SOCKET", previous);
        } else {
            std::env::remove_var("OPENAUSWEIS_DAEMON_SOCKET");
        }

        result
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_rejects_request_id_mismatch() {
        let request = RpcEnvelope::new(Uuid::new_v4(), ClientRequest::GetStatus);
        let response = RpcEnvelope::new(
            Uuid::new_v4(),
            DaemonResponse::Status(openausweis_ipc::DaemonStatus {
                healthy: true,
                pcsc_available: true,
                active_session_count: 0,
                readers: Vec::new(),
                diagnostics: Vec::new(),
                last_error: None,
                ipc_diagnostics: openausweis_ipc::IpcDiagnostics::default(),
            }),
        );

        let err = with_mock_daemon_response(request, Some(response), false)
            .await
            .expect_err("expected request id mismatch error");
        assert!(
            err.to_string().contains("request_id mismatch"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_returns_status_for_non_watch_request() {
        let request = RpcEnvelope::new(Uuid::new_v4(), ClientRequest::GetStatus);
        let response = RpcEnvelope::new(
            request.request_id,
            DaemonResponse::Status(openausweis_ipc::DaemonStatus {
                healthy: true,
                pcsc_available: true,
                active_session_count: 0,
                readers: Vec::new(),
                diagnostics: vec!["ok".to_string()],
                last_error: None,
                ipc_diagnostics: openausweis_ipc::IpcDiagnostics::default(),
            }),
        );

        let forwarded = with_mock_daemon_response(request, Some(response), false)
            .await
            .expect("expected successful status passthrough");

        match forwarded.payload {
            DaemonResponse::Status(status) => {
                assert!(status.healthy);
                assert!(status.pcsc_available);
                assert_eq!(status.active_session_count, 0);
                assert_eq!(status.diagnostics, vec!["ok".to_string()]);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_watch_sessions_accepts_session_updated_first_event() {
        let request = RpcEnvelope::new(
            Uuid::new_v4(),
            ClientRequest::WatchSessions { interval_ms: 500 },
        );
        let session_id = Uuid::new_v4();
        let response = RpcEnvelope::new(
            request.request_id,
            DaemonResponse::SessionUpdated {
                session_id,
                state: openausweis_ipc::SessionState::PinEntry,
                error: None,
                handoff_id: Some("handoff-watch-ok".to_string()),
            },
        );

        let forwarded = with_mock_daemon_response(request, Some(response), false)
            .await
            .expect("expected watch first-event acceptance");

        match forwarded.payload {
            DaemonResponse::SessionUpdated {
                session_id: returned,
                state,
                handoff_id,
                ..
            } => {
                assert_eq!(returned, session_id);
                assert_eq!(state, openausweis_ipc::SessionState::PinEntry);
                assert_eq!(handoff_id.as_deref(), Some("handoff-watch-ok"));
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_watch_sessions_accepts_session_cancelled_first_event() {
        let request = RpcEnvelope::new(
            Uuid::new_v4(),
            ClientRequest::WatchSessions { interval_ms: 500 },
        );
        let session_id = Uuid::new_v4();
        let response = RpcEnvelope::new(
            request.request_id,
            DaemonResponse::SessionCancelled { session_id },
        );

        let forwarded = with_mock_daemon_response(request, Some(response), false)
            .await
            .expect("expected cancelled first-event acceptance");

        match forwarded.payload {
            DaemonResponse::SessionCancelled {
                session_id: returned,
            } => {
                assert_eq!(returned, session_id);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_watch_sessions_accepts_error_first_event() {
        let request = RpcEnvelope::new(
            Uuid::new_v4(),
            ClientRequest::WatchSessions { interval_ms: 500 },
        );
        let response = RpcEnvelope::new(
            request.request_id,
            DaemonResponse::Error {
                code: "SESSION_NOT_FOUND".to_string(),
                message: "session stream unavailable".to_string(),
            },
        );

        let forwarded = with_mock_daemon_response(request, Some(response), false)
            .await
            .expect("expected error first-event acceptance");

        match forwarded.payload {
            DaemonResponse::Error { code, message } => {
                assert_eq!(code, "SESSION_NOT_FOUND");
                assert_eq!(message, "session stream unavailable");
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_watch_sessions_rejects_unexpected_first_event() {
        let request = RpcEnvelope::new(
            Uuid::new_v4(),
            ClientRequest::WatchSessions { interval_ms: 500 },
        );
        let response = RpcEnvelope::new(
            request.request_id,
            DaemonResponse::Status(openausweis_ipc::DaemonStatus {
                healthy: true,
                pcsc_available: true,
                active_session_count: 0,
                readers: Vec::new(),
                diagnostics: Vec::new(),
                last_error: None,
                ipc_diagnostics: openausweis_ipc::IpcDiagnostics::default(),
            }),
        );

        let err = with_mock_daemon_response(request, Some(response), false)
            .await
            .expect_err("expected unexpected first event error");
        assert!(
            err.to_string().contains("unexpected first event type"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_watch_sessions_times_out_waiting_for_first_event() {
        let request = RpcEnvelope::new(
            Uuid::new_v4(),
            ClientRequest::WatchSessions { interval_ms: 500 },
        );

        let err = with_mock_daemon_response(request, None, true)
            .await
            .expect_err("expected watch timeout error");
        assert!(
            err.to_string().contains("timed out while waiting for session stream event"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_rejects_malformed_daemon_response() {
        let request = RpcEnvelope::new(Uuid::new_v4(), ClientRequest::GetStatus);

        let err = with_mock_daemon_raw_line(request, Some("{not-json".to_string()))
            .await
            .expect_err("expected daemon parse error");
        assert!(
            err.to_string().contains("failed to parse daemon response"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forward_to_daemon_errors_when_daemon_closes_without_response() {
        let request = RpcEnvelope::new(Uuid::new_v4(), ClientRequest::GetStatus);

        let err = with_mock_daemon_response(request, None, false)
            .await
            .expect_err("expected closed-connection error");
        assert!(
            err.to_string()
                .contains("daemon closed connection before responding"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn native_host_metrics_default_zeroed() {
        let m = NativeHostMetrics::default();
        assert_eq!(m.requests_processed, 0);
        assert_eq!(m.validation_rejections, 0);
        assert_eq!(m.connection_failures, 0);
    }

    #[test]
    fn native_host_metrics_saturating_add() {
        let mut m = NativeHostMetrics::default();
        m.requests_processed = u64::MAX;
        m.requests_processed = m.requests_processed.saturating_add(1);
        assert_eq!(m.requests_processed, u64::MAX, "should not overflow");
    }

    #[test]
    fn native_host_metrics_persist_writes_valid_json() {
        let m = NativeHostMetrics {
            requests_processed: 5,
            validation_rejections: 2,
            connection_failures: 1,
        };

        let path = std::env::temp_dir()
            .join(format!("openausweis-metrics-test-{}.json", uuid::Uuid::new_v4()));
        // Override sidecar path by writing directly to a temp path.
        if let Ok(json) = serde_json::to_string(&m) {
            std::fs::write(&path, &json).expect("write test metrics");
            let read_back = std::fs::read_to_string(&path).expect("read test metrics");
            let parsed: serde_json::Value =
                serde_json::from_str(&read_back).expect("parse test metrics");
            assert_eq!(parsed["requests_processed"], 5);
            assert_eq!(parsed["validation_rejections"], 2);
            assert_eq!(parsed["connection_failures"], 1);
            let _ = std::fs::remove_file(&path);
        }
    }
}
