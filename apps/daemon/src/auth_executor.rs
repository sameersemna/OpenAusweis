use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{sleep, timeout, Duration};
use uuid::Uuid;

const ENV_AUTH_EXECUTOR: &str = "OPENAUSWEIS_AUTH_EXECUTOR";
const ENV_AA2_BRIDGE_BIN: &str = "OPENAUSWEIS_AUSWEISAPP2_BRIDGE_BIN";
const ENV_AA2_BRIDGE_ARGS: &str = "OPENAUSWEIS_AUSWEISAPP2_BRIDGE_ARGS";
const ENV_AA2_BRIDGE_TIMEOUT_MS: &str = "OPENAUSWEIS_AUSWEISAPP2_BRIDGE_TIMEOUT_MS";
const DEFAULT_AA2_BRIDGE_TIMEOUT_MS: u64 = 20_000;

#[derive(Debug, Serialize)]
struct AuthBridgeRequest {
    protocol_version: u8,
    action: &'static str,
    session_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct AuthBridgeResponse {
    ok: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum AuthExecutorKind {
    Mock,
    AusweisApp2,
    #[cfg(test)]
    ForcedError,
}

#[derive(Debug)]
pub struct AuthExecutor {
    kind: AuthExecutorKind,
    #[cfg(test)]
    forced_error: Option<String>,
}

impl AuthExecutor {
    #[cfg(test)]
    pub fn mock() -> Self {
        Self {
            kind: AuthExecutorKind::Mock,
            forced_error: None,
        }
    }

    #[cfg(test)]
    pub fn fail_for_tests(message: impl Into<String>) -> Self {
        Self {
            kind: AuthExecutorKind::ForcedError,
            forced_error: Some(message.into()),
        }
    }

    pub fn from_env() -> Self {
        let raw = std::env::var(ENV_AUTH_EXECUTOR).unwrap_or_else(|_| "mock".to_string());
        let kind = match raw.trim().to_ascii_lowercase().as_str() {
            "ausweisapp2" => AuthExecutorKind::AusweisApp2,
            _ => AuthExecutorKind::Mock,
        };

        Self {
            kind,
            #[cfg(test)]
            forced_error: None,
        }
    }

    pub async fn execute(&self, session_id: Uuid) -> Result<()> {
        match self.kind {
            AuthExecutorKind::Mock => {
                // Placeholder for real PACE/EAC work.
                let delay = if cfg!(test) { 5 } else { 1200 };
                sleep(Duration::from_millis(delay)).await;
                Ok(())
            }
            AuthExecutorKind::AusweisApp2 => run_ausweisapp2_delegate(session_id).await,
            #[cfg(test)]
            AuthExecutorKind::ForcedError => Err(anyhow!(
                "{}",
                self.forced_error
                    .as_deref()
                    .unwrap_or("forced auth executor failure")
            )),
        }
    }
}

async fn run_ausweisapp2_delegate(session_id: Uuid) -> Result<()> {
    ensure_ausweisapp2_available().await?;

    let bridge_bin = std::env::var(ENV_AA2_BRIDGE_BIN).map_err(|_| {
        anyhow!(
            "{} is not set. Configure an AusweisApp2 bridge command for delegated auth.",
            ENV_AA2_BRIDGE_BIN
        )
    })?;

    let bridge_args = std::env::var(ENV_AA2_BRIDGE_ARGS)
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();

    let timeout_ms = std::env::var(ENV_AA2_BRIDGE_TIMEOUT_MS)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_AA2_BRIDGE_TIMEOUT_MS);

    execute_bridge_command(
        session_id,
        &bridge_bin,
        &bridge_args,
        Duration::from_millis(timeout_ms),
    )
    .await
}

async fn ensure_ausweisapp2_available() -> Result<()> {
    let output = Command::new("ausweisapp2")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .await
        .context("failed to execute ausweisapp2")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ausweisapp2 --version failed with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    Ok(())
}

async fn execute_bridge_command(
    session_id: Uuid,
    bridge_bin: &str,
    bridge_args: &[String],
    bridge_timeout: Duration,
) -> Result<()> {
    let mut command = Command::new(bridge_bin);
    command
        .args(bridge_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch bridge command: {bridge_bin}"))?;

    let request = AuthBridgeRequest {
        protocol_version: 1,
        action: "authenticate",
        session_id,
    };

    let serialized_request = serde_json::to_string(&request).context("serialize bridge request")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(serialized_request.as_bytes())
            .await
            .context("write bridge request")?;
        stdin.write_all(b"\n").await.context("flush request newline")?;
        stdin.shutdown().await.context("close bridge stdin")?;
    }

    let output = timeout(bridge_timeout, child.wait_with_output())
        .await
        .map_err(|_| anyhow!("bridge command timed out after {} ms", bridge_timeout.as_millis()))?
        .context("failed waiting for bridge command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(anyhow!(
            "bridge command failed with status {} stderr='{}' stdout='{}'",
            output.status,
            stderr,
            stdout
        ));
    }

    parse_bridge_response(&output.stdout)
}

fn parse_bridge_response(stdout: &[u8]) -> Result<()> {
    let raw = String::from_utf8_lossy(stdout);
    let response_line = raw
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| anyhow!("bridge response was empty"))?;

    let response: AuthBridgeResponse =
        serde_json::from_str(response_line).context("invalid bridge response JSON")?;

    if response.ok {
        return Ok(());
    }

    Err(anyhow!(
        "{}",
        response
            .error
            .unwrap_or_else(|| "bridge reported authentication failure".to_string())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session_id() -> Uuid {
        Uuid::parse_str("11111111-1111-4111-8111-111111111111").expect("valid uuid")
    }

    #[test]
    fn parse_bridge_response_accepts_ok_payload() {
        let payload = br#"{"ok":true}"#;
        parse_bridge_response(payload).expect("ok payload should pass");
    }

    #[test]
    fn parse_bridge_response_rejects_failure_payload() {
        let payload = br#"{"ok":false,"error":"denied"}"#;
        let err = parse_bridge_response(payload).expect_err("failure payload should fail");
        assert!(err.to_string().contains("denied"));
    }

    #[tokio::test]
    async fn execute_bridge_command_accepts_successful_delegate() {
        let args = vec![
            "-c".to_string(),
            "read line; printf '{\"ok\":true}\n'".to_string(),
        ];

        execute_bridge_command(test_session_id(), "sh", &args, Duration::from_secs(1))
            .await
            .expect("bridge should succeed");
    }

    #[tokio::test]
    async fn execute_bridge_command_surfaces_declined_auth() {
        let args = vec![
            "-c".to_string(),
            "read line; printf '{\"ok\":false,\"error\":\"declined\"}\n'".to_string(),
        ];

        let err = execute_bridge_command(test_session_id(), "sh", &args, Duration::from_secs(1))
            .await
            .expect_err("bridge should return failure");
        assert!(err.to_string().contains("declined"));
    }
}
