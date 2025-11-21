//! CLI inference command over AdapterOS UDS

use adapteros_lora_worker::memory::UmaPressureMonitor;
use anyhow::Result;
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;

/// Run a local inference against the worker UDS server
pub async fn run(
    adapter: Option<String>,
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
    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()?;

    let request_body = serde_json::to_string(&serde_json::json!({
        "cpid": "cli-infer",
        "prompt": prompt,
        "max_tokens": max_tokens.unwrap_or(128),
        "require_evidence": require_evidence,
        "request_type": "normal"
    }))?;

    // Unix socket URL
    let url = format!(
        "http+unix:///{} /api/v1/infer",
        socket.display().to_string().replace(' ', "%20")
    ); // escape if needed
    let url =
        reqwest::Url::parse(&url).map_err(|e| anyhow::anyhow!("Invalid socket URL: {}", e))?;

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(request_body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Inference request failed: {}", e))?;

    if !response.status().is_success() {
        eprintln!("Server error: {}", response.status());
        std::process::exit(1);
    }

    let json: Value = response
        .json()
        .await
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
