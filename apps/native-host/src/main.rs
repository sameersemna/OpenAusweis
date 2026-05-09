use anyhow::{Context, Result};
use openausweis_ipc::{ClientRequest, DaemonResponse, RpcEnvelope, IPC_PROTOCOL_VERSION};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use url::Url;
use uuid::Uuid;

const DAEMON_SOCKET_PATH: &str = "/tmp/openausweis-daemon.sock";

const DEFAULT_ALLOWED_EXACT_ORIGINS: &[&str] = &["http://localhost", "https://localhost"];
const DEFAULT_ALLOWED_SUFFIXES: &[&str] = &[".bundid.de", ".bund.de"];

struct OriginPolicy {
    allowed_exact_origins: HashSet<String>,
    allowed_suffixes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OriginPolicyFile {
    allowed_exact_origins: Vec<String>,
    allowed_suffixes: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    loop {
        let message = match read_native_message(&mut stdin) {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(err) => {
                emit_error(
                    &mut stdout,
                    None,
                    "INVALID_REQUEST",
                    &format!("Invalid native message frame: {err}"),
                )?;
                continue;
            }
        };

        let parsed: RpcEnvelope<ClientRequest> = match serde_json::from_slice(&message) {
            Ok(value) => value,
            Err(err) => {
                emit_error(
                    &mut stdout,
                    None,
                    "INVALID_REQUEST",
                    &format!("Invalid request JSON: {err}"),
                )?;
                continue;
            }
        };

        if parsed.protocol_version != IPC_PROTOCOL_VERSION {
            emit_error(
                &mut stdout,
                Some(parsed.request_id),
                "UNSUPPORTED_PROTOCOL",
                &format!(
                    "protocol {} is unsupported; expected {}",
                    parsed.protocol_version, IPC_PROTOCOL_VERSION
                ),
            )?;
            continue;
        }

        if let Err(err) = validate_client_request(&parsed.payload) {
            emit_error(
                &mut stdout,
                Some(parsed.request_id),
                "RP_NOT_ALLOWED",
                &format!("{err}"),
            )?;
            continue;
        }

        let request_id = parsed.request_id;

        match forward_to_daemon(parsed).await {
            Ok(response) => {
                if response.protocol_version != IPC_PROTOCOL_VERSION {
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

                emit_response(&mut stdout, &response)?;
            }
            Err(err) => emit_error(
                &mut stdout,
                Some(request_id),
                "DAEMON_UNAVAILABLE",
                &format!("{err}"),
            )?,
        }
    }

    Ok(())
}

fn validate_client_request(request: &ClientRequest) -> Result<()> {
    let policy = load_origin_policy();
    match request {
        ClientRequest::StartSession { relying_party } => {
            if !is_origin_allowed(relying_party, &policy)? {
                return Err(anyhow::anyhow!(
                    "relying party origin is not allowed: {relying_party}"
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
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
            exact = policy_file.allowed_exact_origins;
        }

        if !policy_file.allowed_suffixes.is_empty() {
            suffixes = policy_file.allowed_suffixes;
        }
    }

    if let Some(values) = std::env::var("OPENAUSWEIS_ALLOWED_EXACT_ORIGINS")
        .ok()
        .map(parse_csv_list)
        .filter(|values| !values.is_empty())
    {
        exact = values;
    }

    if let Some(values) = std::env::var("OPENAUSWEIS_ALLOWED_SUFFIXES")
        .ok()
        .map(parse_csv_list)
        .filter(|values| !values.is_empty())
    {
        suffixes = values;
    }

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
    let stream = UnixStream::connect(DAEMON_SOCKET_PATH)
        .await
        .with_context(|| format!("failed to connect to daemon socket at {DAEMON_SOCKET_PATH}"))?;

    let (reader, mut writer) = stream.into_split();
    let mut daemon_lines = BufReader::new(reader).lines();

    let encoded = serde_json::to_string(&request).context("failed to encode daemon request")?;
    writer
        .write_all(encoded.as_bytes())
        .await
        .context("failed to write daemon request")?;
    writer
        .write_all(b"\n")
        .await
        .context("failed to write daemon request newline")?;

    let line = daemon_lines
        .next_line()
        .await
        .context("failed to read daemon response")?
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection before responding"))?;

    let response: RpcEnvelope<DaemonResponse> =
        serde_json::from_str(&line).context("failed to parse daemon response")?;

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
