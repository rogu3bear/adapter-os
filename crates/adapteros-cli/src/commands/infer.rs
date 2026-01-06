//! CLI inference command over AdapterOS UDS

use crate::error_codes::{get, ECode};
use adapteros_client::UdsClient;
use adapteros_lora_worker::memory::UmaPressureMonitor;
use anyhow::Result;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

const INFER_API_PATH: &str = "/api/v1/infer";

pub(crate) fn uds_infer_url_string(socket_path: &Path) -> String {
    let socket_display = socket_path.display().to_string();
    let socket_display = socket_display.trim();
    format!("http+unix:///{}{}", socket_display, INFER_API_PATH)
}

/// Run a local inference against the worker UDS server
#[allow(clippy::too_many_arguments)]
pub async fn run(
    _adapter: Option<String>,
    prompt: String,
    max_tokens: Option<usize>,
    require_evidence: bool,
    socket: PathBuf,
    timeout_ms: u64,
    show_citations: bool,
    show_trace: bool,
) -> Result<()> {
    // Backpressure check
    let monitor = UmaPressureMonitor::new(15, None);
    if let Err(e) = monitor.check_headroom() {
        eprintln!(
            "System under pressure: {}. Retry in 30s or reduce max_tokens.",
            e
        );
        std::process::exit(1);
    }

    // Prepare request
    let tenant_id = std::env::var("AOS_TENANT_ID").unwrap_or_else(|_| "default".to_string());
    let request_body = serde_json::to_string(&serde_json::json!({
        "cpid": tenant_id,
        "prompt": prompt,
        "max_tokens": max_tokens.unwrap_or(128),
        "require_evidence": require_evidence,
        "request_type": "normal",
        "determinism_mode": "best_effort",
        "strict_mode": false
    }))?;

    // Unix socket URL
    let url = uds_infer_url_string(&socket);
    tracing::debug!(uds_url = %url, "Using UDS infer URL");
    let uds_client = UdsClient::new(Duration::from_millis(timeout_ms));
    let response_body = match uds_client
        .send_request(&socket, "POST", INFER_API_PATH, Some(&request_body))
        .await
    {
        Ok(body) => body,
        Err(e) => {
            let err_str = e.to_string();
            // Check for common actionable errors
            if err_str.contains("connection refused")
                || err_str.contains("No such file")
                || err_str.contains("ENOENT")
            {
                let error_code = get(ECode::E7003);
                eprintln!(
                    "\x1b[1;31mError {}: {}\x1b[0m\n\n\
                     \x1b[1mCause:\x1b[0m {}\n\n\
                     \x1b[1mFix:\x1b[0m\n{}\n\n\
                     Socket path: {}",
                    error_code.code,
                    error_code.title,
                    error_code.cause,
                    error_code.fix,
                    socket.display()
                );
                std::process::exit(33); // WorkerNotResponding exit code
            }
            return Err(anyhow::anyhow!("Inference request failed: {}", e));
        }
    };

    let json: Value = serde_json::from_str(&response_body)
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    if let Some(text) = json["text"].as_str() {
        println!("{}", text);
    }

    if show_citations {
        if let Some(evidence) = json["trace"]["evidence"].as_array() {
            println!("Citations:");
            for ev in evidence {
                println!(
                    " - {} (score: {}) [{}:{}]",
                    ev["doc_id"], ev["score"], ev["rev"], ev["span_hash"]
                );
            }
        }
    }

    if show_trace {
        println!("Trace: {:?}", json["trace"]);
    }

    if let Some(refusal) = json["refusal"].as_object() {
        eprintln!(
            "Refused: {} (reason: {:?})",
            refusal["message"], refusal["reason"]
        );
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn uds_infer_url_has_expected_shape() {
        let url = uds_infer_url_string(Path::new("./var/run/worker.sock"));
        let parsed = reqwest::Url::parse(&url).expect("uds url should parse");
        assert!(
            !url.contains(' '),
            "expected no literal spaces in url: {}",
            url
        );
        assert!(
            !parsed.as_str().contains("%20"),
            "expected no percent-encoded spaces in url: {}",
            parsed
        );
        assert!(
            parsed.path().ends_with("/api/v1/infer"),
            "expected infer path suffix, got: {}",
            parsed
        );
    }
}
