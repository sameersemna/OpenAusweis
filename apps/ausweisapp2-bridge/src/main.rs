use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

const DEFAULT_WS_URL: &str = "ws://127.0.0.1:24727";
const DEFAULT_TIMEOUT_MS: u64 = 15_000;

const ENV_WS_URL: &str = "OPENAUSWEIS_AA2_WS_URL";
const ENV_TIMEOUT_MS: &str = "OPENAUSWEIS_AA2_WS_TIMEOUT_MS";
const ENV_AUTH_REQUEST: &str = "OPENAUSWEIS_AA2_WS_AUTH_REQUEST";
const ENV_STRICT_SUCCESS: &str = "OPENAUSWEIS_AA2_STRICT_SUCCESS";
const ENV_SUCCESS_MAJORS: &str = "OPENAUSWEIS_AA2_SUCCESS_MAJORS";
const ENV_DIAGNOSTICS: &str = "OPENAUSWEIS_AA2_DIAGNOSTICS";

const DEFAULT_SUCCESS_MAJORS: &[&str] = &["ACCESS_RIGHTS", "ACCEPTED", "AUTH", "AUTHENTICATED"];

#[derive(Debug, Deserialize)]
struct BridgeRequest {
    protocol_version: u8,
    action: String,
    session_id: Uuid,
}

#[derive(Debug, Serialize)]
struct BridgeResponse {
    ok: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct SuccessPolicy {
    strict: bool,
    allowed_majors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AusweisApp2Envelope {
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default)]
    major: Option<String>,
    #[serde(default)]
    minor: Option<String>,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    result: Option<AusweisApp2Result>,
}

#[derive(Debug, Deserialize)]
struct AusweisApp2Result {
    #[serde(default)]
    major: Option<String>,
    #[serde(default)]
    minor: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<Value>,
}

#[tokio::main]
async fn main() {
    let response = run().await;
    if let Err(err) = emit_response(response) {
        let _ = writeln!(io::stderr(), "bridge emit error: {err}");
    }
}

async fn run() -> Result<()> {
    let request = read_request_line().context("read bridge request")?;

    if request.protocol_version != 1 {
        return Err(anyhow!(
            "unsupported protocol_version {} (expected 1)",
            request.protocol_version
        ));
    }

    if request.action != "authenticate" {
        return Err(anyhow!("unsupported action '{}': expected 'authenticate'", request.action));
    }

    let ws_url = std::env::var(ENV_WS_URL).unwrap_or_else(|_| DEFAULT_WS_URL.to_string());
    let timeout_ms = std::env::var(ENV_TIMEOUT_MS)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    let success_policy = load_success_policy();

    let mut ws_request = std::env::var(ENV_AUTH_REQUEST).unwrap_or_else(|_| {
        "{\"cmd\":\"GET_INFO\",\"session_id\":\"{session_id}\"}".to_string()
    });
    ws_request = ws_request.replace("{session_id}", &request.session_id.to_string());
    maybe_log_startup_diagnostics(&ws_url, timeout_ms, &ws_request, &success_policy);

    authenticate_via_ws(
        &ws_url,
        ws_request,
        Duration::from_millis(timeout_ms),
        &success_policy,
    )
        .await
        .with_context(|| format!("websocket auth bridge failed against {ws_url}"))
}

fn maybe_log_startup_diagnostics(
    ws_url: &str,
    timeout_ms: u64,
    ws_request: &str,
    success_policy: &SuccessPolicy,
) {
    let diagnostics_enabled = std::env::var(ENV_DIAGNOSTICS)
        .ok()
        .map(|raw| parse_bool_flag(&raw))
        .unwrap_or(false);

    if !diagnostics_enabled {
        return;
    }

    let line = format_startup_diagnostics_line(ws_url, timeout_ms, ws_request, success_policy);
    let _ = writeln!(io::stderr(), "{line}");
}

fn format_startup_diagnostics_line(
    ws_url: &str,
    timeout_ms: u64,
    ws_request: &str,
    success_policy: &SuccessPolicy,
) -> String {
    format!(
        "bridge diagnostics: ws_url={ws_url} timeout_ms={timeout_ms} strict_success={} success_majors={} ws_request_bytes={}",
        success_policy.strict,
        success_policy.allowed_majors.join(","),
        ws_request.len()
    )
}

fn load_success_policy() -> SuccessPolicy {
    let strict = std::env::var(ENV_STRICT_SUCCESS)
        .ok()
        .map(|raw| parse_bool_flag(&raw))
        .unwrap_or(false);

    let allowed_majors = std::env::var(ENV_SUCCESS_MAJORS)
        .ok()
        .map(|raw| parse_csv_uppercase(&raw))
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_SUCCESS_MAJORS
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        });

    SuccessPolicy {
        strict,
        allowed_majors,
    }
}

fn parse_bool_flag(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn parse_csv_uppercase(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .collect::<Vec<_>>()
}

fn read_request_line() -> Result<BridgeRequest> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();
    let bytes = reader.read_line(&mut line).context("failed reading stdin")?;
    if bytes == 0 {
        return Err(anyhow!("empty stdin"));
    }

    serde_json::from_str::<BridgeRequest>(line.trim()).context("invalid request json")
}

async fn authenticate_via_ws(
    url: &str,
    request_payload: String,
    wait: Duration,
    success_policy: &SuccessPolicy,
) -> Result<()> {
    let (mut ws_stream, _response) = timeout(wait, connect_async(url))
        .await
        .map_err(|_| anyhow!("connect timeout after {} ms", wait.as_millis()))?
        .with_context(|| format!("failed websocket connect to {url}"))?;

    timeout(wait, ws_stream.send(Message::Text(request_payload.into())))
        .await
        .map_err(|_| anyhow!("send timeout after {} ms", wait.as_millis()))?
        .context("failed sending websocket request")?;

    let next_message = timeout(wait, ws_stream.next())
        .await
        .map_err(|_| anyhow!("response timeout after {} ms", wait.as_millis()))?
        .ok_or_else(|| anyhow!("websocket closed without response"))?;

    let message = next_message.context("failed reading websocket response")?;
    match message {
        Message::Text(payload) => validate_ws_response(&payload, success_policy),
        Message::Binary(payload) => {
            let text = String::from_utf8(payload.to_vec()).context("binary response was not utf-8")?;
            validate_ws_response(&text, success_policy)
        }
        Message::Close(frame) => Err(anyhow!(
            "websocket closed before auth completion: {:?}",
            frame.map(|f| f.reason.to_string())
        )),
        other => Err(anyhow!("unexpected websocket frame: {other:?}")),
    }
}

fn validate_ws_response(payload: &str, success_policy: &SuccessPolicy) -> Result<()> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("received empty websocket response"));
    }

    let envelope: AusweisApp2Envelope =
        serde_json::from_str(trimmed).context("invalid AusweisApp2 response JSON")?;

    if envelope.ok == Some(false) {
        return Err(anyhow!(
            "ausweisapp2 response indicated failure: {}",
            extract_error_message(&envelope)
        ));
    }

    if is_error_major(envelope.major.as_deref()) {
        return Err(anyhow!(
            "ausweisapp2 response indicated failure: {}",
            extract_error_message(&envelope)
        ));
    }

    if let Some(result) = envelope.result.as_ref() {
        if is_error_major(result.major.as_deref()) {
            return Err(anyhow!(
                "ausweisapp2 response indicated failure: {}",
                extract_error_message(&envelope)
            ));
        }

        if has_error_value(result.error.as_ref()) {
            return Err(anyhow!(
                "ausweisapp2 response indicated failure: {}",
                extract_error_message(&envelope)
            ));
        }
    }

    if has_error_value(envelope.error.as_ref()) {
        return Err(anyhow!(
            "ausweisapp2 response indicated failure: {}",
            extract_error_message(&envelope)
        ));
    }

    if success_policy.strict && !has_success_marker(&envelope, success_policy) {
        return Err(anyhow!(
            "ausweisapp2 response did not contain an allowlisted success marker"
        ));
    }

    Ok(())
}

fn has_success_marker(envelope: &AusweisApp2Envelope, success_policy: &SuccessPolicy) -> bool {
    if envelope.ok == Some(true) {
        return true;
    }

    let major_matches = |value: Option<&str>| {
        value
            .map(|major| major.trim().to_ascii_uppercase())
            .map(|major| success_policy.allowed_majors.iter().any(|allowed| allowed == &major))
            .unwrap_or(false)
    };

    if major_matches(envelope.major.as_deref()) {
        return true;
    }

    if let Some(result) = envelope.result.as_ref() {
        if major_matches(result.major.as_deref()) {
            return true;
        }
    }

    major_matches(envelope.msg.as_deref())
}

fn is_error_major(major: Option<&str>) -> bool {
    major
        .map(|value| value.trim().eq_ignore_ascii_case("error"))
        .unwrap_or(false)
}

fn has_error_value(value: Option<&Value>) -> bool {
    match value {
        None => false,
        Some(Value::Null) => false,
        Some(Value::Bool(false)) => false,
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
        Some(_) => true,
    }
}

fn extract_error_message(envelope: &AusweisApp2Envelope) -> String {
    if let Some(result) = envelope.result.as_ref() {
        if let Some(message) = result.message.as_deref() {
            if !message.trim().is_empty() {
                return message.trim().to_string();
            }
        }

        if let Some(minor) = result.minor.as_deref() {
            if !minor.trim().is_empty() {
                return minor.trim().to_string();
            }
        }

        if let Some(err) = stringify_error_value(result.error.as_ref()) {
            return err;
        }
    }

    if let Some(minor) = envelope.minor.as_deref() {
        if !minor.trim().is_empty() {
            return minor.trim().to_string();
        }
    }

    if let Some(msg) = envelope.msg.as_deref() {
        if !msg.trim().is_empty() {
            return msg.trim().to_string();
        }
    }

    if let Some(err) = stringify_error_value(envelope.error.as_ref()) {
        return err;
    }

    "ausweisapp2 returned an unspecified error".to_string()
}

fn stringify_error_value(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::Null => None,
        Value::Bool(false) => None,
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        other => Some(other.to_string()),
    }
}

fn emit_response(result: Result<()>) -> Result<()> {
    let response = match result {
        Ok(()) => BridgeResponse {
            ok: true,
            error: None,
        },
        Err(err) => BridgeResponse {
            ok: false,
            error: Some(err.to_string()),
        },
    };

    let encoded = serde_json::to_string(&response).context("serialize response")?;
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout
        .write_all(encoded.as_bytes())
        .context("write response")?;
    stdout.write_all(b"\n").context("write newline")?;
    stdout.flush().context("flush stdout")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    fn permissive_policy() -> SuccessPolicy {
        SuccessPolicy {
            strict: false,
            allowed_majors: DEFAULT_SUCCESS_MAJORS
                .iter()
                .map(|value| value.to_string())
                .collect(),
        }
    }

    fn strict_policy() -> SuccessPolicy {
        SuccessPolicy {
            strict: true,
            allowed_majors: DEFAULT_SUCCESS_MAJORS
                .iter()
                .map(|value| value.to_string())
                .collect(),
        }
    }

    #[test]
    fn format_startup_diagnostics_line_includes_policy_and_transport_fields() {
        let policy = strict_policy();
        let line = format_startup_diagnostics_line(
            "ws://127.0.0.1:24727",
            15000,
            "{\"cmd\":\"GET_INFO\"}",
            &policy,
        );

        assert!(line.contains("ws_url=ws://127.0.0.1:24727"));
        assert!(line.contains("timeout_ms=15000"));
        assert!(line.contains("strict_success=true"));
        assert!(line.contains("success_majors=ACCESS_RIGHTS,ACCEPTED,AUTH,AUTHENTICATED"));
        assert!(line.contains("ws_request_bytes="));
    }

    async fn spawn_mock_ws_server(response_payload: &'static str) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock websocket listener");
        let address = listener.local_addr().expect("listener local_addr");
        let ws_url = format!("ws://{}/", address);

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

        let task = tokio::spawn(async move {
            // Signal that we are now polling accept() so the client knows it is
            // safe to connect. The send happens-before accept() is polled, which
            // is fine: the OS kernel has already placed the listener socket into
            // LISTEN state (done by TcpListener::bind), so the client's TCP
            // connect will succeed regardless. Explicitly signalling here
            // avoids the parallel-test timing window that caused flaky failures.
            let _ = ready_tx.send(());

            let (stream, _) = listener.accept().await.expect("accept mock websocket client");
            let mut ws = accept_async(stream).await.expect("upgrade websocket");

            let incoming = ws.next().await.expect("missing client frame").expect("frame read error");
            match incoming {
                Message::Text(payload) => {
                    assert!(payload.contains("\"cmd\""), "request payload should contain cmd field");
                }
                other => panic!("unexpected request frame: {other:?}"),
            }

            ws.send(Message::Text(response_payload.to_string().into()))
                .await
                .expect("send websocket response");
        });

        // Wait for the server task to reach its accept() poll before returning.
        ready_rx.await.expect("mock server ready signal dropped");

        (ws_url, task)
    }

    #[test]
    fn validate_ws_response_accepts_non_error_payload() {
        validate_ws_response("{\"msg\":\"ACCESS_RIGHTS\"}", &permissive_policy())
            .expect("expected successful payload");
    }

    #[test]
    fn validate_ws_response_rejects_error_payload() {
        let err =
            validate_ws_response("{\"major\":\"error\",\"minor\":\"x\"}", &permissive_policy())
                .expect_err("expected failure payload");
        assert!(err.to_string().contains("indicated failure"));
    }

    #[test]
    fn validate_ws_response_rejects_nested_result_error() {
        let err = validate_ws_response(
            "{\"result\":{\"major\":\"error\",\"message\":\"denied\"}}",
            &permissive_policy(),
        )
        .expect_err("expected nested error payload");
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn validate_ws_response_rejects_error_field_object() {
        let err = validate_ws_response("{\"error\":{\"message\":\"failed\"}}", &permissive_policy())
            .expect_err("expected error object payload");
        assert!(err.to_string().contains("failed"));
    }

    #[test]
    fn validate_ws_response_strict_mode_rejects_unknown_success_marker() {
        let err = validate_ws_response("{\"msg\":\"PING\"}", &strict_policy())
            .expect_err("strict mode should reject unknown marker");
        assert!(err
            .to_string()
            .contains("did not contain an allowlisted success marker"));
    }

    #[test]
    fn validate_ws_response_strict_mode_accepts_allowlisted_marker() {
        validate_ws_response("{\"major\":\"ACCESS_RIGHTS\"}", &strict_policy())
            .expect("strict mode should accept allowlisted marker");
    }

    #[tokio::test]
    async fn authenticate_via_ws_accepts_success_payload_from_mock_server() {
        let (ws_url, task) = spawn_mock_ws_server("{\"msg\":\"ACCESS_RIGHTS\"}").await;

        authenticate_via_ws(
            &ws_url,
            "{\"cmd\":\"GET_INFO\",\"session_id\":\"123\"}".to_string(),
            Duration::from_secs(2),
            &strict_policy(),
        )
        .await
        .expect("expected successful websocket bridge auth");

        task.await.expect("mock server task should complete");
    }

    #[tokio::test]
    async fn authenticate_via_ws_surfaces_protocol_error_from_mock_server() {
        let (ws_url, task) =
            spawn_mock_ws_server("{\"major\":\"error\",\"minor\":\"PIN_INVALID\"}").await;

        let err = authenticate_via_ws(
            &ws_url,
            "{\"cmd\":\"GET_INFO\",\"session_id\":\"123\"}".to_string(),
            Duration::from_secs(2),
            &strict_policy(),
        )
        .await
        .expect_err("expected websocket bridge auth failure");
        let message = err.to_string();
        assert!(
            message.contains("PIN_INVALID") || message.contains("indicated failure"),
            "unexpected error message: {message}"
        );

        task.await.expect("mock server task should complete");
    }
}
