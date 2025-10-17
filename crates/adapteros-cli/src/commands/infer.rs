//! CLI inference command over AdapterOS UDS

use anyhow::{Context, Result};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

/// Run a local inference against the worker UDS server
pub async fn run(
    adapter: Option<String>,
    prompt: String,
    max_tokens: Option<usize>,
    require_evidence: bool,
    socket: PathBuf,
    timeout_ms: u64,
) -> Result<()> {
    // Build request
    let request = json!({
        "cpid": "local_dev", // for local calls; server path sets real tenant
        "prompt": prompt,
        "max_tokens": max_tokens.unwrap_or(128),
        "require_evidence": require_evidence,
    });

    // UDS client (simple HTTP over UDS)
    let client = adapteros_client::UdsClient::new(Duration::from_millis(timeout_ms));

    // Optionally stage/swap adapter before inference via /adapter JSON endpoint
    if let Some(adapter_id) = adapter {
        // Preload
        let preload_body = serde_json::to_string(&json!({
            "type": "preload",
            "adapter_id": adapter_id,
            // Hash is unknown from CLI; worker accepts placeholder in current API
            "hash": adapteros_core::B3Hash::hash(b"placeholder"),
        }))?;

        let _ = client
            .send_request(&socket, "POST", "/adapter", Some(&preload_body))
            .await
            .context("Failed to preload adapter")?;

        // Swap (activate)
        let swap_body = serde_json::to_string(&json!({
            "type": "swap",
            "add_ids": [adapter_id],
            "remove_ids": [] as [String; 0],
        }))?;

        let _ = client
            .send_request(&socket, "POST", "/adapter", Some(&swap_body))
            .await
            .context("Failed to swap adapter")?;
    }

    let body = serde_json::to_string(&request)?;
    let resp = client
        .send_request(&socket, "POST", "/inference", Some(&body))
        .await
        .context("Inference request failed")?;

    // Parse worker response and print text only
    let v: serde_json::Value =
        serde_json::from_str(&resp).context("Failed to parse response JSON")?;
    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        println!("{}", text);
    } else {
        println!("{}", resp);
    }

    Ok(())
}
