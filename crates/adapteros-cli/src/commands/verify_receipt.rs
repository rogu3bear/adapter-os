//! Offline receipt verification for run evidence bundles.
//!
//! This command delegates all verification logic to `adapteros-crypto` so the
//! server and CLI cannot drift.

use std::fs;
use std::path::{Path, PathBuf};

use adapteros_crypto::{verify_bundle_bytes, ReceiptVerificationReport, VerifyOptions};
use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::http_client::send_with_refresh_from_store;
use crate::output::OutputWriter;

const DEFAULT_BUNDLE_FILENAMES: &[&str] = &[
    "receipt_bundle.json",
    "run_receipt.json",
    "inference_trace.json",
];

// Re-export schema version constants for backward compatibility.
pub use adapteros_core::receipt_digest::{
    RECEIPT_SCHEMA_CURRENT as RECEIPT_SCHEMA_CURRENT_REEXPORT,
    RECEIPT_SCHEMA_V1 as RECEIPT_SCHEMA_V1_REEXPORT,
    RECEIPT_SCHEMA_V2 as RECEIPT_SCHEMA_V2_REEXPORT,
    RECEIPT_SCHEMA_V3 as RECEIPT_SCHEMA_V3_REEXPORT,
    RECEIPT_SCHEMA_V4 as RECEIPT_SCHEMA_V4_REEXPORT,
    RECEIPT_SCHEMA_V5 as RECEIPT_SCHEMA_V5_REEXPORT,
    RECEIPT_SCHEMA_V6 as RECEIPT_SCHEMA_V6_REEXPORT,
    RECEIPT_SCHEMA_V7 as RECEIPT_SCHEMA_V7_REEXPORT,
};

fn resolve_bundle_path(bundle: &Path) -> Result<PathBuf> {
    if bundle.is_file() {
        return Ok(bundle.to_path_buf());
    }
    for candidate in DEFAULT_BUNDLE_FILENAMES {
        let path = bundle.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    bail!(
        "Bundle not found. Expected one of {:?} inside {}",
        DEFAULT_BUNDLE_FILENAMES,
        bundle.display()
    )
}

fn load_bundle_bytes(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("Failed to read bundle file {}", path.display()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnlineDigestDiff {
    field: String,
    expected_hex: String,
    computed_hex: String,
    matches: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnlineReceiptVerificationResult {
    trace_id: String,
    #[serde(default)]
    tenant_id: Option<String>,
    source: String,
    pass: bool,
    verified_at: String,
    #[serde(default)]
    reasons: Vec<String>,
    #[serde(default)]
    mismatched_token: Option<u32>,
    context_digest: OnlineDigestDiff,
    run_head_hash: OnlineDigestDiff,
    output_digest: OnlineDigestDiff,
    receipt_digest: OnlineDigestDiff,
    signature_checked: bool,
    #[serde(default)]
    signature_valid: Option<bool>,
}

fn base_url_candidates(raw: &str) -> Vec<String> {
    let trimmed = raw.trim_end_matches('/').trim().to_string();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    out.push(trimmed.clone());

    if trimmed.ends_with("/api") {
        let stripped = trimmed
            .trim_end_matches("/api")
            .trim_end_matches('/')
            .to_string();
        if !stripped.is_empty() {
            out.push(stripped);
        }
    } else {
        out.push(format!("{trimmed}/api"));
    }

    out.sort();
    out.dedup();
    out
}

async fn fetch_online_trace_verification(
    trace_id: &str,
    server_url: &str,
) -> Result<OnlineReceiptVerificationResult> {
    let client = Client::builder().build()?;
    let candidates = base_url_candidates(server_url);
    if candidates.is_empty() {
        bail!("server url is empty");
    }

    let body = serde_json::json!({ "trace_id": trace_id });

    for base in candidates {
        let url = format!("{}/v1/replay/verify/trace", base);

        // Try with stored auth (for protected deployments); fall back to anonymous request.
        let response = match send_with_refresh_from_store(&client, |c, auth| {
            let auth_base = auth.base_url.trim_end_matches('/');
            let target = if server_url.trim().is_empty() {
                format!("{}/v1/replay/verify/trace", auth_base)
            } else {
                url.clone()
            };
            c.post(target).bearer_auth(&auth.token).json(&body)
        })
        .await
        {
            Ok(resp) => resp,
            Err(_) => client.post(&url).json(&body).send().await?,
        };

        let status = response.status();
        let bytes = response.bytes().await.unwrap_or_default();

        if status.as_u16() == 404 {
            // Common local mismatch: some environments use /api prefix, others don't.
            continue;
        }

        if !status.is_success() {
            bail!(
                "failed to verify trace receipt for {}: {} {}",
                trace_id,
                status,
                String::from_utf8_lossy(&bytes)
            );
        }

        let report: OnlineReceiptVerificationResult = serde_json::from_slice(&bytes)
            .with_context(|| "Failed to parse trace verification response")?;
        return Ok(report);
    }

    bail!(
        "trace verification endpoint not found at {} (tried /v1/replay/verify/trace with and without /api)",
        server_url
    )
}

fn render_report(report: &ReceiptVerificationReport, output: &OutputWriter) -> Result<()> {
    output.json(report)?;
    if output.is_json() {
        return Ok(());
    }

    if report.pass {
        output.success("Verification passed");
    } else {
        output.print("Verification failed");
    }

    output.kv("Trace ID", &report.trace_id);
    if let Some(tenant) = report.tenant_id.as_ref() {
        output.kv("Tenant", tenant);
    }

    output.kv(
        "Context Digest",
        if report.context_digest.matches {
            "match"
        } else {
            "mismatch"
        },
    );
    output.kv(
        "Run Head Hash",
        if report.run_head_hash.matches {
            "match"
        } else {
            "mismatch"
        },
    );
    output.kv(
        "Output Digest",
        if report.output_digest.matches {
            "match"
        } else {
            "mismatch"
        },
    );
    output.kv(
        "Receipt Digest",
        if report.receipt_digest.matches {
            "match"
        } else {
            "mismatch"
        },
    );

    if !report.reasons.is_empty() {
        output.kv(
            "Reasons",
            &report
                .reasons
                .iter()
                .map(|r| r.as_str())
                .collect::<Vec<_>>()
                .join(","),
        );
    }

    if report.signature_checked {
        output.kv(
            "Signature",
            match report.signature_valid {
                Some(true) => "valid",
                Some(false) => "invalid",
                None => "skipped",
            },
        );
    }

    if let Some(token) = report.mismatched_token {
        output.kv("First mismatched token", &token.to_string());
    }

    Ok(())
}

fn render_online_report(
    report: &OnlineReceiptVerificationResult,
    output: &OutputWriter,
) -> Result<()> {
    output.json(report)?;
    if output.is_json() {
        return Ok(());
    }

    if report.pass {
        output.success("Verification passed");
    } else {
        output.print("Verification failed");
    }

    output.kv("Trace ID", &report.trace_id);
    if let Some(tenant) = report.tenant_id.as_ref() {
        output.kv("Tenant", tenant);
    }

    output.kv(
        "Context Digest",
        if report.context_digest.matches {
            "match"
        } else {
            "mismatch"
        },
    );
    output.kv(
        "Run Head Hash",
        if report.run_head_hash.matches {
            "match"
        } else {
            "mismatch"
        },
    );
    output.kv(
        "Output Digest",
        if report.output_digest.matches {
            "match"
        } else {
            "mismatch"
        },
    );
    output.kv(
        "Receipt Digest",
        if report.receipt_digest.matches {
            "match"
        } else {
            "mismatch"
        },
    );

    if !report.reasons.is_empty() {
        output.kv("Reasons", &report.reasons.join(","));
    }

    if report.signature_checked {
        output.kv(
            "Signature",
            match report.signature_valid {
                Some(true) => "valid",
                Some(false) => "invalid",
                None => "skipped",
            },
        );
    }

    if let Some(token) = report.mismatched_token {
        output.kv("First mismatched token", &token.to_string());
    }

    Ok(())
}

/// Parse a hex string into a 32-byte seed array.
pub fn parse_seed_hex(hex_str: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_str).context("Invalid hex encoding for expected seed")?;
    if bytes.len() != 32 {
        bail!(
            "Expected seed must be 32 bytes (64 hex chars), got {} bytes",
            bytes.len()
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

pub async fn run(
    bundle: Option<&Path>,
    online_trace: Option<&str>,
    server_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    run_with_seed_options(bundle, online_trace, server_url, output, None, false, None).await
}

/// Run receipt verification with seed verification options.
///
/// # Arguments
/// * `bundle` - Path to receipt bundle file
/// * `online_trace` - Trace ID for online verification
/// * `server_url` - Server URL for online verification
/// * `output` - Output writer
/// * `expected_seed_hex` - Optional hex-encoded expected seed (64 chars)
/// * `require_seed_digest` - Require seed digest in receipt (fail if missing)
/// * `expected_seed_mode` - Optional expected seed mode ("strict", "best_effort")
pub async fn run_with_seed_options(
    bundle: Option<&Path>,
    online_trace: Option<&str>,
    server_url: &str,
    output: &OutputWriter,
    expected_seed_hex: Option<&str>,
    require_seed_digest: bool,
    expected_seed_mode: Option<&str>,
) -> Result<()> {
    if bundle.is_none() && online_trace.is_none() {
        bail!("provide --bundle or --online <trace_id> to verify a receipt");
    }

    let expected_seed = match expected_seed_hex {
        Some(hex) => Some(parse_seed_hex(hex)?),
        None => None,
    };

    let options = VerifyOptions {
        expected_seed,
        require_seed_digest,
        expected_seed_mode: expected_seed_mode.map(|s| s.to_string()),
    };

    if let Some(trace_id) = online_trace {
        if output.is_verbose() {
            output.progress(format!(
                "Verifying receipt for trace {} via {}",
                trace_id, server_url
            ));
        }

        let report = fetch_online_trace_verification(trace_id, server_url).await?;
        render_online_report(&report, output)?;

        if !report.pass || !report.reasons.is_empty() {
            bail!(format!(
                "receipt verification failed: {}",
                if report.reasons.is_empty() {
                    "UNKNOWN".to_string()
                } else {
                    report.reasons.join(",")
                }
            ));
        }

        return Ok(());
    };

    let bundle_path = resolve_bundle_path(
        bundle.expect("BUG: bundle path argument must be provided when --online-trace is not set"),
    )?;
    let bytes = load_bundle_bytes(&bundle_path)?;

    let report = verify_bundle_bytes(&bytes, &options)?;

    render_report(&report, output)?;

    if !report.reasons.is_empty() {
        bail!(format!(
            "receipt verification failed: {}",
            report
                .reasons
                .iter()
                .map(|r| r.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }

    Ok(())
}
