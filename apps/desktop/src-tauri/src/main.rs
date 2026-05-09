#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use openausweis_ipc::{ClientRequest, DaemonResponse, RpcEnvelope, IPC_PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::Path;
use std::path::PathBuf;
use tauri::{CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use uuid::Uuid;

const DAEMON_SOCKET_PATH: &str = "/tmp/openausweis-daemon.sock";
const DEFAULT_ALLOWED_EXACT_ORIGINS: &[&str] = &["http://localhost", "https://localhost"];
const DEFAULT_ALLOWED_SUFFIXES: &[&str] = &[".bundid.de", ".bund.de"];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopDaemonStatus {
    healthy: bool,
    pcsc_available: bool,
    active_session_count: u32,
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
    let stream = UnixStream::connect(DAEMON_SOCKET_PATH)
        .await
        .with_context(|| format!("failed to connect to daemon socket at {DAEMON_SOCKET_PATH}"))?;

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let request_id = Uuid::new_v4();
    let request = RpcEnvelope::new(request_id, ClientRequest::GetStatus);

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

    if response.protocol_version != IPC_PROTOCOL_VERSION {
        return Err(anyhow::anyhow!(
            "daemon protocol mismatch: expected {}, got {}",
            IPC_PROTOCOL_VERSION,
            response.protocol_version
        ));
    }

    if response.request_id != request_id {
        return Err(anyhow::anyhow!(
            "daemon returned mismatched request id: expected {}, got {}",
            request_id,
            response.request_id
        ));
    }

    match response.payload {
        DaemonResponse::Status(status) => Ok(DesktopDaemonStatus {
            healthy: status.healthy,
            pcsc_available: status.pcsc_available,
            active_session_count: status.active_session_count,
        }),
        DaemonResponse::Error { code, message } => {
            Err(anyhow::anyhow!("daemon error {code}: {message}"))
        }
        other => Err(anyhow::anyhow!("unexpected daemon response: {other:?}")),
    }
}

fn main() {
    let show = CustomMenuItem::new("show", "Show OpenAusweis");
    let quit = CustomMenuItem::new("quit", "Quit");
    let tray_menu = SystemTrayMenu::new().add_item(show).add_item(quit);

    tauri::Builder::default()
        .system_tray(SystemTray::new().with_menu(tray_menu))
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } if id == "show" => {
                if let Some(window) = app.get_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            SystemTrayEvent::MenuItemClick { id, .. } if id == "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            probe_daemon_status,
            get_origin_policy,
            save_origin_policy
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri app");
}
