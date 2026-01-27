//! AdapterOS Smoke Test Runner
//!
//! Validates inference correctness and receipt verification:
//! - Health and readiness checks
//! - Non-stream inference (thinking off/on)
//! - Stream inference (thinking off/on)
//! - Receipt capture and verification

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Smoke test runner for AdapterOS
#[derive(Parser, Debug)]
#[command(name = "aos-smoke", version, about)]
struct Args {
    /// Base URL of the AdapterOS server
    #[arg(short, long, default_value = "http://localhost:8080")]
    base_url: String,

    /// API key for authentication (optional)
    #[arg(short, long, env = "AOS_API_KEY")]
    api_key: Option<String>,

    /// Timeout in seconds for each request
    #[arg(short, long, default_value = "30")]
    timeout: u64,

    /// Run in verbose mode
    #[arg(short, long)]
    verbose: bool,

    /// Only run health checks
    #[arg(long)]
    health_only: bool,

    /// Output results as JSON
    #[arg(long)]
    json: bool,
}

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Serialize)]
struct InferRequest {
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct InferResponse {
    id: Option<String>,
    text: Option<String>,
    #[serde(default)]
    tokens_generated: usize,
    finish_reason: Option<String>,
    latency_ms: Option<u64>,
    #[serde(default)]
    adapters_used: Vec<String>,
    run_receipt: Option<RunReceipt>,
    backend_used: Option<String>,
    fallback_backend: Option<String>,
    #[serde(default)]
    fallback_triggered: bool,
    trace: Option<InferenceTrace>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunReceipt {
    receipt_digest: Option<String>,
    run_head_hash: Option<String>,
    output_digest: Option<String>,
    logical_prompt_tokens: Option<u32>,
    logical_output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct InferenceTrace {
    trace_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct TraceVerifyRequest {
    trace_id: String,
}

#[derive(Debug, Deserialize)]
struct ReceiptVerificationResult {
    trace_id: String,
    pass: bool,
    verified_at: Option<String>,
    #[serde(default)]
    reasons: Vec<String>,
    context_digest: Option<ReceiptDigestDiff>,
    run_head_hash: Option<ReceiptDigestDiff>,
    output_digest: Option<ReceiptDigestDiff>,
    receipt_digest: Option<ReceiptDigestDiff>,
    #[serde(default)]
    signature_checked: bool,
    signature_valid: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ReceiptDigestDiff {
    field: String,
    expected_hex: Option<String>,
    computed_hex: Option<String>,
    #[serde(default)]
    matches: bool,
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: Option<String>,
    #[serde(default)]
    healthy: bool,
}

#[derive(Debug, Deserialize)]
struct ReadyResponse {
    #[serde(default)]
    ready: bool,
    status: Option<String>,
}

// ============================================================================
// Test Results
// ============================================================================

#[derive(Debug, Clone, Serialize)]
struct TestResult {
    name: String,
    passed: bool,
    duration_ms: u64,
    trace_id: Option<String>,
    receipt_verified: Option<bool>,
    error: Option<String>,
    details: Option<String>,
}

#[derive(Debug, Serialize)]
struct SmokeTestResults {
    passed: bool,
    total: usize,
    passed_count: usize,
    failed_count: usize,
    results: Vec<TestResult>,
    trace_ids: Vec<String>,
}

// ============================================================================
// HTTP Client
// ============================================================================

struct SmokeClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    verbose: bool,
}

impl SmokeClient {
    fn new(base_url: &str, api_key: Option<String>, timeout: Duration, verbose: bool) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            verbose,
        }
    }

    fn log(&self, msg: &str) {
        if self.verbose {
            eprintln!("{}", msg.dimmed());
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        self.log(&format!("GET {}{}", self.base_url, path));

        let mut req = self.client.get(format!("{}{}", self.base_url, path));

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await.context("Request failed")?;
        let status = resp.status();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {} - {}", status, body);
        }

        resp.json().await.context("Failed to parse response")
    }

    async fn post<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R> {
        self.log(&format!("POST {}{}", self.base_url, path));

        let mut req = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .json(body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await.context("Request failed")?;
        let status = resp.status();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {} - {}", status, body);
        }

        resp.json().await.context("Failed to parse response")
    }

    async fn post_stream(&self, path: &str, body: &InferRequest) -> Result<(String, Option<String>)> {
        self.log(&format!("POST (stream) {}{}", self.base_url, path));

        let mut req = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .json(body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req = req.header("Accept", "text/event-stream");

        let resp = req.send().await.context("Request failed")?;
        let status = resp.status();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {} - {}", status, body);
        }

        // Parse SSE events to extract final result
        let body = resp.text().await?;
        let mut output_text = String::new();
        let mut trace_id = None;

        for line in body.lines() {
            if line.starts_with("data:") {
                let data = line.trim_start_matches("data:").trim();
                if data == "[DONE]" {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                    // Token event
                    if let Some(token) = event.get("token").and_then(|t| t.as_str()) {
                        output_text.push_str(token);
                    }
                    // Done event with trace_id
                    if let Some(tid) = event.get("trace_id").and_then(|t| t.as_str()) {
                        trace_id = Some(tid.to_string());
                    }
                }
            }
        }

        Ok((output_text, trace_id))
    }
}

// ============================================================================
// Test Functions
// ============================================================================

async fn test_health(client: &SmokeClient) -> TestResult {
    let start = std::time::Instant::now();
    let name = "health_check".to_string();

    match client.get::<HealthResponse>("/healthz").await {
        Ok(resp) => TestResult {
            name,
            passed: resp.healthy || resp.status.as_deref() == Some("ok"),
            duration_ms: start.elapsed().as_millis() as u64,
            trace_id: None,
            receipt_verified: None,
            error: None,
            details: Some(format!("status: {:?}", resp.status)),
        },
        Err(e) => TestResult {
            name,
            passed: false,
            duration_ms: start.elapsed().as_millis() as u64,
            trace_id: None,
            receipt_verified: None,
            error: Some(e.to_string()),
            details: None,
        },
    }
}

async fn test_ready(client: &SmokeClient) -> TestResult {
    let start = std::time::Instant::now();
    let name = "ready_check".to_string();

    match client.get::<ReadyResponse>("/readyz").await {
        Ok(resp) => TestResult {
            name,
            passed: resp.ready || resp.status.as_deref() == Some("ready"),
            duration_ms: start.elapsed().as_millis() as u64,
            trace_id: None,
            receipt_verified: None,
            error: None,
            details: Some(format!("status: {:?}", resp.status)),
        },
        Err(e) => TestResult {
            name,
            passed: false,
            duration_ms: start.elapsed().as_millis() as u64,
            trace_id: None,
            receipt_verified: None,
            error: Some(e.to_string()),
            details: None,
        },
    }
}

async fn test_infer_non_stream(
    client: &SmokeClient,
    thinking: bool,
    seed: u64,
) -> TestResult {
    let start = std::time::Instant::now();
    let name = format!(
        "infer_non_stream_thinking_{}",
        if thinking { "on" } else { "off" }
    );

    let req = InferRequest {
        prompt: "What is 2+2? Answer with just the number.".to_string(),
        reasoning_mode: Some(thinking),
        seed: Some(seed),
        max_tokens: Some(50),
        temperature: Some(0.0),
    };

    match client.post::<_, InferResponse>("/v1/infer", &req).await {
        Ok(resp) => {
            let trace_id = resp
                .trace
                .as_ref()
                .and_then(|t| t.trace_id.clone())
                .or_else(|| resp.id.clone());

            let has_output = resp.text.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
            let has_receipt = resp.run_receipt.is_some();

            TestResult {
                name,
                passed: has_output && resp.error.is_none(),
                duration_ms: start.elapsed().as_millis() as u64,
                trace_id,
                receipt_verified: None,
                error: resp.error,
                details: Some(format!(
                    "tokens: {}, backend: {:?}, receipt: {}",
                    resp.tokens_generated,
                    resp.backend_used,
                    has_receipt
                )),
            }
        }
        Err(e) => TestResult {
            name,
            passed: false,
            duration_ms: start.elapsed().as_millis() as u64,
            trace_id: None,
            receipt_verified: None,
            error: Some(e.to_string()),
            details: None,
        },
    }
}

async fn test_infer_stream(client: &SmokeClient, thinking: bool, seed: u64) -> TestResult {
    let start = std::time::Instant::now();
    let name = format!(
        "infer_stream_thinking_{}",
        if thinking { "on" } else { "off" }
    );

    let req = InferRequest {
        prompt: "What is 3+3? Answer with just the number.".to_string(),
        reasoning_mode: Some(thinking),
        seed: Some(seed),
        max_tokens: Some(50),
        temperature: Some(0.0),
    };

    match client.post_stream("/v1/infer/stream", &req).await {
        Ok((text, trace_id)) => {
            let has_output = !text.is_empty();

            TestResult {
                name,
                passed: has_output,
                duration_ms: start.elapsed().as_millis() as u64,
                trace_id,
                receipt_verified: None,
                error: None,
                details: Some(format!("output_len: {}", text.len())),
            }
        }
        Err(e) => TestResult {
            name,
            passed: false,
            duration_ms: start.elapsed().as_millis() as u64,
            trace_id: None,
            receipt_verified: None,
            error: Some(e.to_string()),
            details: None,
        },
    }
}

async fn test_receipt_verify(client: &SmokeClient, trace_id: &str) -> TestResult {
    let start = std::time::Instant::now();
    let name = format!("receipt_verify_{}", &trace_id[..8.min(trace_id.len())]);

    let req = TraceVerifyRequest {
        trace_id: trace_id.to_string(),
    };

    match client
        .post::<_, ReceiptVerificationResult>("/v1/replay/verify/trace", &req)
        .await
    {
        Ok(resp) => {
            let reasons = if resp.reasons.is_empty() {
                None
            } else {
                Some(resp.reasons.join(", "))
            };

            TestResult {
                name,
                passed: resp.pass,
                duration_ms: start.elapsed().as_millis() as u64,
                trace_id: Some(trace_id.to_string()),
                receipt_verified: Some(resp.pass),
                error: reasons,
                details: Some(format!(
                    "signature_checked: {}, signature_valid: {:?}",
                    resp.signature_checked, resp.signature_valid
                )),
            }
        }
        Err(e) => TestResult {
            name,
            passed: false,
            duration_ms: start.elapsed().as_millis() as u64,
            trace_id: Some(trace_id.to_string()),
            receipt_verified: Some(false),
            error: Some(e.to_string()),
            details: None,
        },
    }
}

// ============================================================================
// Main Runner
// ============================================================================

async fn run_smoke_tests(args: &Args) -> SmokeTestResults {
    let client = SmokeClient::new(
        &args.base_url,
        args.api_key.clone(),
        Duration::from_secs(args.timeout),
        args.verbose,
    );

    let mut results = Vec::new();
    let mut trace_ids = Vec::new();

    // Health checks
    println!("{}", "Running health checks...".cyan());
    results.push(test_health(&client).await);
    results.push(test_ready(&client).await);

    if args.health_only {
        let passed_count = results.iter().filter(|r| r.passed).count();
        let failed_count = results.len() - passed_count;
        return SmokeTestResults {
            passed: failed_count == 0,
            total: results.len(),
            passed_count,
            failed_count,
            results,
            trace_ids,
        };
    }

    // Inference tests
    println!("{}", "Running inference tests...".cyan());
    let seed = 42u64;

    // Non-stream, thinking off
    let r = test_infer_non_stream(&client, false, seed).await;
    if let Some(ref tid) = r.trace_id {
        trace_ids.push(tid.clone());
    }
    results.push(r);

    // Non-stream, thinking on
    let r = test_infer_non_stream(&client, true, seed + 1).await;
    if let Some(ref tid) = r.trace_id {
        trace_ids.push(tid.clone());
    }
    results.push(r);

    // Stream, thinking off
    let r = test_infer_stream(&client, false, seed + 2).await;
    if let Some(ref tid) = r.trace_id {
        trace_ids.push(tid.clone());
    }
    results.push(r);

    // Stream, thinking on
    let r = test_infer_stream(&client, true, seed + 3).await;
    if let Some(ref tid) = r.trace_id {
        trace_ids.push(tid.clone());
    }
    results.push(r);

    // Receipt verification for each trace
    println!("{}", "Verifying receipts...".cyan());
    for tid in &trace_ids.clone() {
        let r = test_receipt_verify(&client, tid).await;
        results.push(r);
    }

    let passed_count = results.iter().filter(|r| r.passed).count();
    let failed_count = results.len() - passed_count;

    SmokeTestResults {
        passed: failed_count == 0,
        total: results.len(),
        passed_count,
        failed_count,
        results,
        trace_ids,
    }
}

fn print_results(results: &SmokeTestResults) {
    println!();
    println!("{}", "═══════════════════════════════════════════════════════".bold());
    println!("{}", " SMOKE TEST RESULTS".bold());
    println!("{}", "═══════════════════════════════════════════════════════".bold());
    println!();

    for r in &results.results {
        let status = if r.passed {
            "PASS".green().bold()
        } else {
            "FAIL".red().bold()
        };

        println!(
            "  {} {} ({}ms)",
            status,
            r.name,
            r.duration_ms
        );

        if let Some(ref details) = r.details {
            println!("      {}", details.dimmed());
        }

        if let Some(ref error) = r.error {
            println!("      {}: {}", "error".red(), error);
        }

        if let Some(ref tid) = r.trace_id {
            println!("      trace_id: {}", tid.dimmed());
        }
    }

    println!();
    println!("{}", "───────────────────────────────────────────────────────".dimmed());

    let summary = format!(
        " Total: {} | Passed: {} | Failed: {}",
        results.total, results.passed_count, results.failed_count
    );

    if results.passed {
        println!("{}", summary.green().bold());
    } else {
        println!("{}", summary.red().bold());
    }

    if !results.trace_ids.is_empty() {
        println!();
        println!("{}", "Trace IDs:".cyan());
        for tid in &results.trace_ids {
            println!("  - {}", tid);
        }
    }

    println!();
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let results = run_smoke_tests(&args).await;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        print_results(&results);
    }

    if results.passed {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
