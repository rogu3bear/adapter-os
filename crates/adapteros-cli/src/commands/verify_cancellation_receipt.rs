//! Verify cancellation receipts for audit trail completeness.
//!
//! Cancellation receipts provide cryptographic proof of partial inference output
//! when an inference is cancelled (client disconnect, timeout, manual cancel).
//! This command verifies:
//! - Receipt digest integrity (all bound fields hash correctly)
//! - Optional Ed25519 signature verification against expected public key

use std::fs;
use std::path::Path;

use adapteros_core::{B3Hash, CancelSource, CancellationReceipt};
use adapteros_crypto::signature::PublicKey;
use adapteros_db::Db;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::output::OutputWriter;

/// Verification result for JSON output
#[derive(Debug, Serialize)]
struct VerificationReport {
    trace_id: String,
    receipt_digest: String,
    partial_output_count: u32,
    cancellation_source: String,
    cancelled_at_token: u32,
    digest_valid: bool,
    signature_present: bool,
    signature_valid: Option<bool>,
    tenant_id: Option<String>,
    cancelled_at: Option<String>,
}

/// Load a cancellation receipt from the database by trace ID.
async fn load_receipt_from_db(trace_id: &str) -> Result<CancellationReceipt> {
    let db = Db::connect_env()
        .await
        .context("Failed to connect to database")?;

    let pool = db.pool_opt().ok_or_else(|| {
        anyhow::anyhow!("SQL backend unavailable - cannot load cancellation receipt")
    })?;

    let row = sqlx::query(
        r#"
        SELECT
            trace_id,
            partial_output_digest,
            partial_output_count,
            stop_reason,
            cancellation_source,
            cancelled_at_token,
            receipt_digest,
            signature,
            equipment_profile_digest,
            context_digest,
            tenant_id,
            cancelled_at
        FROM cancellation_receipts
        WHERE trace_id = ?
        LIMIT 1
        "#,
    )
    .bind(trace_id)
    .fetch_optional(pool)
    .await
    .context("Failed to query cancellation_receipts table")?;

    let row = row.ok_or_else(|| {
        anyhow::anyhow!("No cancellation receipt found for trace_id: {}", trace_id)
    })?;

    // Parse fields from database row
    use sqlx::Row;

    let trace_id: String = row.get("trace_id");
    let partial_output_digest_bytes: Vec<u8> = row.get("partial_output_digest");
    let partial_output_count: i64 = row.get("partial_output_count");
    let stop_reason: String = row.get("stop_reason");
    let cancellation_source_str: String = row.get("cancellation_source");
    let cancelled_at_token: i64 = row.get("cancelled_at_token");
    let receipt_digest_bytes: Vec<u8> = row.get("receipt_digest");
    let signature: Option<Vec<u8>> = row.get("signature");
    let equipment_profile_digest_bytes: Option<Vec<u8>> = row.get("equipment_profile_digest");
    let context_digest_bytes: Option<Vec<u8>> = row.get("context_digest");
    let tenant_id: Option<String> = row.get("tenant_id");
    let cancelled_at: Option<String> = row.get("cancelled_at");

    // Convert bytes to B3Hash
    let partial_output_digest = bytes_to_b3hash(&partial_output_digest_bytes)?;
    let receipt_digest = bytes_to_b3hash(&receipt_digest_bytes)?;

    // Parse cancellation source
    let cancellation_source = parse_cancel_source(&cancellation_source_str)?;

    // Parse optional equipment profile (we only have the digest, not the full profile)
    let equipment_profile = equipment_profile_digest_bytes
        .map(|bytes| {
            let digest = bytes_to_b3hash(&bytes)?;
            // Create a minimal equipment profile with just the digest
            // The actual processor_id/engine_version aren't stored in the table
            Ok::<_, anyhow::Error>(adapteros_core::crypto_receipt::EquipmentProfile {
                processor_id: "unknown".to_string(),
                engine_version: "unknown".to_string(),
                ane_version: None,
                digest,
            })
        })
        .transpose()?;

    let context_digest = context_digest_bytes
        .map(|bytes| bytes_to_b3hash(&bytes))
        .transpose()?;

    Ok(CancellationReceipt {
        schema_version: adapteros_core::crypto_receipt::CANCELLATION_RECEIPT_SCHEMA_VERSION,
        trace_id,
        partial_output_digest,
        partial_output_count: partial_output_count as u32,
        stop_reason,
        cancellation_source,
        cancelled_at_token: cancelled_at_token as u32,
        receipt_digest,
        signature,
        equipment_profile,
        context_digest,
        tenant_id,
        cancelled_at,
    })
}

/// Load a cancellation receipt from a JSON file.
fn load_receipt_from_file(path: &Path) -> Result<CancellationReceipt> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read receipt file: {}", path.display()))?;

    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse receipt JSON from: {}", path.display()))
}

/// Verify the receipt's Ed25519 signature against an expected public key.
fn verify_signature(receipt: &CancellationReceipt, expected_pubkey_hex: &str) -> Result<bool> {
    let Some(ref sig_bytes) = receipt.signature else {
        bail!("Receipt has no signature to verify");
    };

    if sig_bytes.len() != 64 {
        bail!(
            "Invalid signature length: expected 64 bytes, got {}",
            sig_bytes.len()
        );
    }

    // Decode the expected public key
    let pubkey_bytes =
        hex::decode(expected_pubkey_hex).context("Failed to decode expected public key hex")?;

    if pubkey_bytes.len() != 32 {
        bail!(
            "Invalid public key length: expected 32 bytes, got {}",
            pubkey_bytes.len()
        );
    }

    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&pubkey_bytes);
    let public_key = PublicKey::from_bytes(&pk_array)
        .map_err(|e| anyhow::anyhow!("Failed to parse public key: {}", e))?;

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(sig_bytes);
    let signature = adapteros_crypto::signature::Signature::from_bytes(&sig_array)
        .map_err(|e| anyhow::anyhow!("Failed to parse signature: {}", e))?;

    // Verify the signature against the receipt digest
    Ok(public_key
        .verify(receipt.receipt_digest.as_bytes(), &signature)
        .is_ok())
}

/// Convert bytes to B3Hash.
fn bytes_to_b3hash(bytes: &[u8]) -> Result<B3Hash> {
    if bytes.len() != 32 {
        bail!(
            "Invalid hash length: expected 32 bytes, got {}",
            bytes.len()
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(bytes);
    Ok(B3Hash::from_bytes(arr))
}

/// Parse a string into CancelSource.
fn parse_cancel_source(s: &str) -> Result<CancelSource> {
    match s.to_lowercase().as_str() {
        "clientdisconnect" | "client_disconnect" => Ok(CancelSource::ClientDisconnect),
        "requesttimeout" | "request_timeout" => Ok(CancelSource::RequestTimeout),
        "manualcancel" | "manual_cancel" => Ok(CancelSource::ManualCancel),
        "policyviolation" | "policy_violation" => Ok(CancelSource::PolicyViolation),
        "resourceexhaustion" | "resource_exhaustion" => Ok(CancelSource::ResourceExhaustion),
        _ => bail!("Unknown cancellation source: {}", s),
    }
}

/// Main entry point for verifying cancellation receipts.
pub async fn run(
    trace_id: Option<&str>,
    file: Option<&Path>,
    expected_pubkey: Option<&str>,
    output: &OutputWriter,
) -> Result<()> {
    // Load receipt from either database or file
    let receipt = match (trace_id, file) {
        (Some(tid), None) => {
            output.info(format!("Loading cancellation receipt for trace: {}", tid));
            load_receipt_from_db(tid).await?
        }
        (None, Some(path)) => {
            output.info(format!(
                "Loading cancellation receipt from: {}",
                path.display()
            ));
            load_receipt_from_file(path)?
        }
        (Some(_), Some(_)) => {
            bail!("Specify either trace_id or --file, not both");
        }
        (None, None) => {
            bail!("Must specify either trace_id or --file");
        }
    };

    // Verify digest integrity
    let digest_valid = receipt.verify();

    // Verify signature if expected pubkey provided
    let (signature_present, signature_valid) = if let Some(pubkey_hex) = expected_pubkey {
        if receipt.signature.is_some() {
            let valid = verify_signature(&receipt, pubkey_hex)?;
            (true, Some(valid))
        } else {
            (false, None)
        }
    } else {
        (receipt.signature.is_some(), None)
    };

    // Build report
    let report = VerificationReport {
        trace_id: receipt.trace_id.clone(),
        receipt_digest: receipt.receipt_digest.to_hex(),
        partial_output_count: receipt.partial_output_count,
        cancellation_source: receipt.cancellation_source.to_string(),
        cancelled_at_token: receipt.cancelled_at_token,
        digest_valid,
        signature_present,
        signature_valid,
        tenant_id: receipt.tenant_id.clone(),
        cancelled_at: receipt.cancelled_at.clone(),
    };

    // Output results
    if output.is_json() {
        output.json(&report)?;
    } else {
        output.section("Cancellation Receipt Verification");
        output.kv("Trace ID", &report.trace_id);
        output.kv("Receipt Digest", &report.receipt_digest);
        output.kv(
            "Partial Output Count",
            &report.partial_output_count.to_string(),
        );
        output.kv("Cancellation Source", &report.cancellation_source);
        output.kv("Cancelled at Token", &report.cancelled_at_token.to_string());

        if let Some(ref tenant) = report.tenant_id {
            output.kv("Tenant ID", tenant);
        }
        if let Some(ref cancelled_at) = report.cancelled_at {
            output.kv("Cancelled At", cancelled_at);
        }

        output.blank();
        if digest_valid {
            output.success("Digest verification: PASS");
        } else {
            output.error("Digest verification: FAIL - receipt may have been tampered");
        }

        if signature_present {
            match signature_valid {
                Some(true) => output.success("Signature verification: PASS"),
                Some(false) => output.error("Signature verification: FAIL - invalid signature"),
                None => output.info("Signature present (provide --expected-pubkey to verify)"),
            }
        } else {
            output.info("No signature attached to receipt");
        }
    }

    // Return error if verification failed
    if !digest_valid {
        bail!("Cancellation receipt verification failed: digest mismatch");
    }

    if signature_valid == Some(false) {
        bail!("Cancellation receipt verification failed: invalid signature");
    }

    Ok(())
}
