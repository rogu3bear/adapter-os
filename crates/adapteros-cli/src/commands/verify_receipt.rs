//! Offline receipt verification for run evidence bundles.
//!
//! Recomputes context/run/output/receipt digests and validates optional signatures.

use std::fs;
use std::path::{Path, PathBuf};

use adapteros_core::B3Hash;
use adapteros_crypto::signature::{PublicKey, Signature};
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http_client::send_with_refresh_from_store;
use crate::output::OutputWriter;

const DEFAULT_BUNDLE_FILENAMES: &[&str] = &[
    "receipt_bundle.json",
    "run_receipt.json",
    "inference_trace.json",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptBundle {
    #[serde(default)]
    version: Option<String>,
    trace_id: String,
    tenant_id: String,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    context_digest_hex: Option<String>,
    context: ReceiptContext,
    tokens: Vec<ReceiptToken>,
    output_tokens: Vec<u32>,
    receipt: ReceiptDigests,
    #[serde(default)]
    expected_backend: Option<String>,
    #[serde(default)]
    expected_kernel_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptContext {
    tenant_namespace: String,
    stack_hash_hex: String,
    prompt_tokens: Vec<u32>,
    #[serde(default)]
    policy_mask_digest_hex: Option<String>,
    #[serde(default)]
    context_digest_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptToken {
    token_index: u32,
    adapter_ids: Vec<String>,
    gates_q15: Vec<i16>,
    #[serde(default)]
    policy_mask_digest_hex: Option<String>,
    #[serde(default)]
    backend_id: Option<String>,
    #[serde(default)]
    kernel_version_id: Option<String>,
    #[serde(default)]
    decision_hash_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptDigests {
    run_head_hash_hex: String,
    output_digest_hex: String,
    receipt_digest_hex: String,
    #[serde(default)]
    signature_b64: Option<String>,
    #[serde(default)]
    public_key_hex: Option<String>,
    #[serde(default)]
    logical_prompt_tokens: u32,
    #[serde(default)]
    prefix_cached_token_count: u32,
    #[serde(default)]
    billed_input_tokens: u32,
    #[serde(default)]
    logical_output_tokens: u32,
    #[serde(default)]
    billed_output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReasonCode {
    ContextMismatch,
    TraceTamper,
    OutputMismatch,
    PolicyMismatch,
    BackendMismatch,
    SignatureInvalid,
}

impl ReasonCode {
    fn as_str(&self) -> &'static str {
        match self {
            ReasonCode::ContextMismatch => "CONTEXT_MISMATCH",
            ReasonCode::TraceTamper => "TRACE_TAMPER",
            ReasonCode::OutputMismatch => "OUTPUT_MISMATCH",
            ReasonCode::PolicyMismatch => "POLICY_MISMATCH",
            ReasonCode::BackendMismatch => "BACKEND_MISMATCH",
            ReasonCode::SignatureInvalid => "SIGNATURE_INVALID",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DigestComparison {
    pub computed: String,
    pub expected: String,
    pub matches: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiptVerificationReport {
    pub trace_id: String,
    pub reasons: Vec<ReasonCode>,
    pub mismatched_token: Option<u32>,
    pub context_digest: DigestComparison,
    pub run_head_hash: DigestComparison,
    pub output_digest: DigestComparison,
    pub receipt_digest: DigestComparison,
    pub signature_checked: bool,
    pub signature_valid: Option<bool>,
}

fn push_reason(reasons: &mut Vec<ReasonCode>, code: ReasonCode) {
    if !reasons.iter().any(|r| r.as_str() == code.as_str()) {
        reasons.push(code);
    }
}

fn decode_hex_32(label: &str, hex: &str) -> Result<[u8; 32]> {
    let bytes =
        hex::decode(hex).with_context(|| format!("Failed to decode {label} hex ({hex})"))?;
    if bytes.len() != 32 {
        bail!("{label} must be 32 bytes, got {}", bytes.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn encode_adapter_ids(ids: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + ids.iter().map(|s| s.len() + 4).sum::<usize>());
    out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
    for id in ids {
        let bytes = id.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn encode_gates_q15(gates: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + gates.len() * 2);
    out.extend_from_slice(&(gates.len() as u32).to_le_bytes());
    for g in gates {
        out.extend_from_slice(&g.to_le_bytes());
    }
    out
}

fn hash_decision(
    context_digest: &[u8; 32],
    token_index: u32,
    adapter_blob: &[u8],
    gates_blob: &[u8],
    policy_mask_digest: Option<[u8; 32]>,
    backend_id: Option<&str>,
    kernel_version_id: Option<&str>,
) -> B3Hash {
    let policy_bytes = policy_mask_digest.map(|d| d.to_vec()).unwrap_or_default();
    let backend_bytes = backend_id.unwrap_or("").as_bytes().to_vec();
    let kernel_bytes = kernel_version_id.unwrap_or("").as_bytes().to_vec();

    B3Hash::hash_multi(&[
        &context_digest[..],
        &token_index.to_le_bytes(),
        &(adapter_blob.len() as u32).to_le_bytes(),
        adapter_blob,
        &(gates_blob.len() as u32).to_le_bytes(),
        gates_blob,
        &(policy_bytes.len() as u32).to_le_bytes(),
        &policy_bytes,
        &(backend_bytes.len() as u32).to_le_bytes(),
        &backend_bytes,
        &(kernel_bytes.len() as u32).to_le_bytes(),
        &kernel_bytes,
    ])
}

fn update_head(prev: &B3Hash, token_index: u32, decision_hash: &B3Hash) -> B3Hash {
    B3Hash::hash_multi(&[
        prev.as_bytes(),
        decision_hash.as_bytes(),
        &token_index.to_le_bytes(),
    ])
}

fn compute_output_digest(output_tokens: &[u32]) -> B3Hash {
    let mut buf = Vec::with_capacity(4 + output_tokens.len() * 4);
    buf.extend_from_slice(&(output_tokens.len() as u32).to_le_bytes());
    for t in output_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    B3Hash::hash(&buf)
}

fn compute_context_digest(ctx: &ReceiptContext) -> Result<B3Hash> {
    let stack_bytes =
        hex::decode(&ctx.stack_hash_hex).with_context(|| "Failed to decode stack_hash_hex")?;
    let mut buf = Vec::with_capacity(
        ctx.tenant_namespace.len() + stack_bytes.len() + 4 + (ctx.prompt_tokens.len() * 4),
    );
    buf.extend_from_slice(ctx.tenant_namespace.as_bytes());
    buf.extend_from_slice(&stack_bytes);
    buf.extend_from_slice(&(ctx.prompt_tokens.len() as u32).to_le_bytes());
    for t in &ctx.prompt_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    Ok(B3Hash::hash(&buf))
}

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

fn parse_bundle_from_str(raw: &str) -> Result<ReceiptBundle> {
    if let Ok(bundle) = serde_json::from_str::<ReceiptBundle>(raw) {
        return Ok(bundle);
    }

    let value: Value = serde_json::from_str(raw)?;
    if let Some(nested) = value
        .get("receipt_bundle")
        .or_else(|| value.get("bundle"))
        .cloned()
    {
        return Ok(serde_json::from_value(nested)?);
    }

    Err(anyhow!("Unexpected receipt payload format"))
}

fn load_bundle(path: &Path) -> Result<ReceiptBundle> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("Failed to read bundle file {}", path.display()))?;
    parse_bundle_from_str(&data)
        .with_context(|| format!("Failed to parse bundle file {}", path.display()))
}

fn verify_signature(
    receipt: &ReceiptDigests,
    receipt_digest: &B3Hash,
    reasons: &mut Vec<ReasonCode>,
) -> Result<(bool, Option<bool>)> {
    let Some(signature_b64) = receipt.signature_b64.as_ref() else {
        return Ok((false, None));
    };
    let Some(pubkey_hex) = receipt.public_key_hex.as_ref() else {
        push_reason(reasons, ReasonCode::SignatureInvalid);
        return Ok((true, Some(false)));
    };

    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(signature_b64) {
        Ok(bytes) => bytes,
        Err(e) => {
            push_reason(reasons, ReasonCode::SignatureInvalid);
            return Err(anyhow!("Invalid base64 signature: {e}"));
        }
    };
    if sig_bytes.len() != 64 {
        push_reason(reasons, ReasonCode::SignatureInvalid);
        return Ok((true, Some(false)));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature =
        Signature::from_bytes(&sig_arr).map_err(|e| anyhow!("Invalid signature bytes: {e}"))?;

    let pub_bytes = decode_hex_32("public_key", pubkey_hex)?;
    let pubkey =
        PublicKey::from_bytes(&pub_bytes).map_err(|e| anyhow!("Invalid public key: {e}"))?;

    let verified = pubkey.verify(receipt_digest.as_bytes(), &signature).is_ok();
    if !verified {
        push_reason(reasons, ReasonCode::SignatureInvalid);
    }

    Ok((true, Some(verified)))
}

fn verify_bundle(bundle: &ReceiptBundle) -> Result<ReceiptVerificationReport> {
    let mut reasons: Vec<ReasonCode> = Vec::new();
    let computed_context = compute_context_digest(&bundle.context)?;

    let expected_context_hex = bundle
        .context_digest_hex
        .as_ref()
        .or(bundle.context.context_digest_hex.as_ref())
        .cloned()
        .unwrap_or_else(|| computed_context.to_hex());
    let context_expected = B3Hash::from_hex(&expected_context_hex)
        .with_context(|| "Invalid expected context digest hex")?;
    if context_expected != computed_context {
        push_reason(&mut reasons, ReasonCode::ContextMismatch);
    }

    let logical_prompt_tokens = bundle.receipt.logical_prompt_tokens;
    let prompt_token_len = bundle.context.prompt_tokens.len() as u32;
    if logical_prompt_tokens != prompt_token_len {
        push_reason(&mut reasons, ReasonCode::ContextMismatch);
    }

    let canonical_billed_input =
        logical_prompt_tokens.saturating_sub(bundle.receipt.prefix_cached_token_count);
    if canonical_billed_input != bundle.receipt.billed_input_tokens {
        push_reason(&mut reasons, ReasonCode::TraceTamper);
    }

    if bundle.receipt.logical_output_tokens != bundle.output_tokens.len() as u32 {
        push_reason(&mut reasons, ReasonCode::TraceTamper);
    }

    if bundle.receipt.billed_output_tokens != bundle.receipt.logical_output_tokens {
        push_reason(&mut reasons, ReasonCode::TraceTamper);
    }

    if let Some(expected_backend) = bundle.expected_backend.as_ref() {
        let expected_backend = expected_backend.to_lowercase();
        if bundle.tokens.iter().any(|t| {
            t.backend_id
                .as_ref()
                .map(|b| b.to_lowercase() != expected_backend)
                .unwrap_or(true)
        }) {
            push_reason(&mut reasons, ReasonCode::BackendMismatch);
        }
    }

    if let Some(expected_kernel) = bundle.expected_kernel_version.as_ref() {
        let expected_kernel = expected_kernel.to_lowercase();
        if bundle.tokens.iter().any(|t| {
            t.kernel_version_id
                .as_ref()
                .map(|k| k.to_lowercase() != expected_kernel)
                .unwrap_or(true)
        }) {
            push_reason(&mut reasons, ReasonCode::BackendMismatch);
        }
    }

    if let Some(expected_policy_hex) = bundle.context.policy_mask_digest_hex.as_ref() {
        let expected_policy = decode_hex_32("policy_mask_digest_hex", expected_policy_hex)?;
        if bundle.tokens.iter().any(|t| {
            t.policy_mask_digest_hex
                .as_ref()
                .and_then(|p| decode_hex_32("policy_mask_digest_hex", p).ok())
                .map(|digest| digest != expected_policy)
                .unwrap_or(true)
        }) {
            push_reason(&mut reasons, ReasonCode::PolicyMismatch);
        }
    }

    let mut run_head = B3Hash::zero();
    let mut mismatched_token = None;

    let mut tokens_sorted = bundle.tokens.clone();
    tokens_sorted.sort_by_key(|t| t.token_index);

    for token in &tokens_sorted {
        let adapter_blob = encode_adapter_ids(&token.adapter_ids);
        let gates_blob = encode_gates_q15(&token.gates_q15);
        let policy_digest = match &token.policy_mask_digest_hex {
            Some(hex) => Some(decode_hex_32("policy_mask_digest_hex", hex)?),
            None => None,
        };
        let decision_hash = hash_decision(
            computed_context.as_bytes(),
            token.token_index,
            &adapter_blob,
            &gates_blob,
            policy_digest,
            token.backend_id.as_deref(),
            token.kernel_version_id.as_deref(),
        );

        if let Some(expected_hash_hex) = token.decision_hash_hex.as_ref() {
            let expected_hash =
                B3Hash::from_hex(expected_hash_hex).with_context(|| "Invalid decision_hash_hex")?;
            if expected_hash != decision_hash && mismatched_token.is_none() {
                mismatched_token = Some(token.token_index);
            }
        }

        run_head = update_head(&run_head, token.token_index, &decision_hash);
    }

    let expected_run_head =
        B3Hash::from_hex(&bundle.receipt.run_head_hash_hex).with_context(|| {
            format!(
                "Invalid run_head_hash_hex ({})",
                bundle.receipt.run_head_hash_hex
            )
        })?;
    if expected_run_head != run_head {
        push_reason(&mut reasons, ReasonCode::TraceTamper);
        mismatched_token.get_or_insert(tokens_sorted.last().map(|t| t.token_index).unwrap_or(0));
    }

    let output_digest = compute_output_digest(&bundle.output_tokens);
    let expected_output = B3Hash::from_hex(&bundle.receipt.output_digest_hex)
        .with_context(|| "Invalid output_digest_hex")?;
    if expected_output != output_digest {
        push_reason(&mut reasons, ReasonCode::OutputMismatch);
    }

    let receipt_digest = B3Hash::hash_multi(&[
        computed_context.as_bytes(),
        run_head.as_bytes(),
        output_digest.as_bytes(),
        &bundle.receipt.logical_prompt_tokens.to_le_bytes(),
        &bundle.receipt.prefix_cached_token_count.to_le_bytes(),
        &bundle.receipt.billed_input_tokens.to_le_bytes(),
        &bundle.receipt.logical_output_tokens.to_le_bytes(),
        &bundle.receipt.billed_output_tokens.to_le_bytes(),
    ]);
    let expected_receipt =
        B3Hash::from_hex(&bundle.receipt.receipt_digest_hex).with_context(|| {
            format!(
                "Invalid receipt_digest_hex ({})",
                bundle.receipt.receipt_digest_hex
            )
        })?;
    if expected_receipt != receipt_digest {
        push_reason(&mut reasons, ReasonCode::TraceTamper);
    }

    let (signature_checked, signature_valid) =
        verify_signature(&bundle.receipt, &expected_receipt, &mut reasons)?;

    let report = ReceiptVerificationReport {
        trace_id: bundle.trace_id.clone(),
        reasons,
        mismatched_token,
        context_digest: DigestComparison {
            computed: computed_context.to_hex(),
            expected: expected_context_hex,
            matches: computed_context == context_expected,
        },
        run_head_hash: DigestComparison {
            computed: run_head.to_hex(),
            expected: bundle.receipt.run_head_hash_hex.clone(),
            matches: run_head == expected_run_head,
        },
        output_digest: DigestComparison {
            computed: output_digest.to_hex(),
            expected: bundle.receipt.output_digest_hex.clone(),
            matches: output_digest == expected_output,
        },
        receipt_digest: DigestComparison {
            computed: receipt_digest.to_hex(),
            expected: bundle.receipt.receipt_digest_hex.clone(),
            matches: receipt_digest == expected_receipt,
        },
        signature_checked,
        signature_valid,
    };

    Ok(report)
}

fn render_report(report: &ReceiptVerificationReport, output: &OutputWriter) -> Result<()> {
    if output.is_json() {
        output.json(report)?;
        return Ok(());
    }

    output.section("Receipt Verification");
    if report.reasons.is_empty() {
        output.success("Verification passed");
    } else {
        output.error(format!(
            "Verification failed: {}",
            report
                .reasons
                .iter()
                .map(|r| r.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ));
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

async fn fetch_online_bundle(trace_id: &str, server_url: &str) -> Result<ReceiptBundle> {
    let client = Client::builder().build()?;
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/v1/trace/{}/receipt", base, trace_id);

    // Try with stored auth (for protected deployments); fall back to anonymous request.
    let response = match send_with_refresh_from_store(&client, |c, auth| {
        let auth_base = auth.base_url.trim_end_matches('/');
        let target = if server_url.is_empty() {
            format!("{}/v1/trace/{}/receipt", auth_base, trace_id)
        } else {
            url.clone()
        };
        c.get(target).bearer_auth(&auth.token)
    })
    .await
    {
        Ok(resp) => resp,
        Err(_) => client.get(&url).send().await?,
    };

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!(
            "failed to fetch receipt for trace {}: {} {}",
            trace_id,
            status,
            body
        );
    }

    parse_bundle_from_str(&body).context("Failed to decode receipt payload")
}

pub async fn run(
    bundle: Option<&Path>,
    online_trace: Option<&str>,
    server_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    if bundle.is_none() && online_trace.is_none() {
        bail!("provide --bundle or --online <trace_id> to verify a receipt");
    }

    let bundle = if let Some(trace_id) = online_trace {
        if output.is_verbose() {
            output.progress(format!(
                "Fetching receipt for trace {} from {}",
                trace_id, server_url
            ));
        }
        fetch_online_bundle(trace_id, server_url).await?
    } else {
        let bundle_path = resolve_bundle_path(
            bundle.expect("bundle path should be present when online_trace is None"),
        )?;
        load_bundle(&bundle_path)?
    };
    let report = verify_bundle(&bundle)?;

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

// Exposed for tests
#[cfg(test)]
pub fn verify_bundle_from_path(bundle: &Path) -> Result<ReceiptVerificationReport> {
    let bundle = load_bundle(bundle)?;
    verify_bundle(&bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_crypto::signature::Keypair;
    use adapteros_platform::common::PlatformUtils;
    use base64::engine::general_purpose::STANDARD;
    use rand::RngCore;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    fn make_keypair() -> Keypair {
        let mut seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut seed);
        Keypair::from_bytes(&seed)
    }

    fn write_bundle(dir: &Path, tamper: bool) -> PathBuf {
        let prompt_tokens = vec![11u32, 22u32, 33u32];
        let stack_hash = B3Hash::hash(b"stack-123").to_hex();
        let context = ReceiptContext {
            tenant_namespace: "tenant-demo".to_string(),
            stack_hash_hex: stack_hash,
            prompt_tokens: prompt_tokens.clone(),
            policy_mask_digest_hex: None,
            context_digest_hex: None,
        };

        let context_digest = compute_context_digest(&context).expect("context digest");
        let token0 = ReceiptToken {
            token_index: 0,
            adapter_ids: vec!["adapter-a".to_string()],
            gates_q15: vec![123],
            policy_mask_digest_hex: None,
            backend_id: Some("coreml".to_string()),
            kernel_version_id: Some("k1".to_string()),
            decision_hash_hex: None,
        };
        let token1 = ReceiptToken {
            token_index: 1,
            adapter_ids: vec!["adapter-b".to_string(), "adapter-c".to_string()],
            gates_q15: vec![321, 111],
            policy_mask_digest_hex: None,
            backend_id: Some("coreml".to_string()),
            kernel_version_id: Some("k1".to_string()),
            decision_hash_hex: None,
        };

        let mut tokens = vec![token0, token1];
        let mut run_head = B3Hash::zero();
        for t in tokens.iter_mut() {
            let adapter_blob = encode_adapter_ids(&t.adapter_ids);
            let gates_blob = encode_gates_q15(&t.gates_q15);
            let decision = hash_decision(
                context_digest.as_bytes(),
                t.token_index,
                &adapter_blob,
                &gates_blob,
                None,
                t.backend_id.as_deref(),
                t.kernel_version_id.as_deref(),
            );
            t.decision_hash_hex = Some(decision.to_hex());
            run_head = update_head(&run_head, t.token_index, &decision);
        }

        let output_tokens = vec![101u32, 102u32, 103u32];
        let output_digest = compute_output_digest(&output_tokens);
        let logical_prompt_tokens = prompt_tokens.len() as u32;
        let prefix_cached_token_count = 0;
        let billed_input_tokens = logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
        let logical_output_tokens = output_tokens.len() as u32;
        let billed_output_tokens = logical_output_tokens;
        let receipt_digest = B3Hash::hash_multi(&[
            context_digest.as_bytes(),
            run_head.as_bytes(),
            output_digest.as_bytes(),
            &logical_prompt_tokens.to_le_bytes(),
            &prefix_cached_token_count.to_le_bytes(),
            &billed_input_tokens.to_le_bytes(),
            &logical_output_tokens.to_le_bytes(),
            &billed_output_tokens.to_le_bytes(),
        ]);

        let keypair = make_keypair();
        let signature = keypair.sign(receipt_digest.as_bytes());
        let signature_b64 = STANDARD.encode(signature.to_bytes());
        let public_key_hex = hex::encode(keypair.public_key().to_bytes());

        let mut bundle = ReceiptBundle {
            version: Some("aos-receipt-v1".to_string()),
            trace_id: "trace-demo".to_string(),
            tenant_id: "tenant-demo".to_string(),
            request_id: Some("req-123".to_string()),
            context_digest_hex: Some(context_digest.to_hex()),
            context,
            tokens,
            output_tokens: output_tokens.clone(),
            receipt: ReceiptDigests {
                run_head_hash_hex: run_head.to_hex(),
                output_digest_hex: output_digest.to_hex(),
                receipt_digest_hex: receipt_digest.to_hex(),
                signature_b64: Some(signature_b64),
                public_key_hex: Some(public_key_hex),
                logical_prompt_tokens,
                prefix_cached_token_count,
                billed_input_tokens,
                logical_output_tokens,
                billed_output_tokens,
            },
            expected_backend: Some("coreml".to_string()),
            expected_kernel_version: Some("k1".to_string()),
        };

        let bundle_path = dir.join("receipt_bundle.json");
        if tamper {
            // Flip one gate to trigger TRACE_TAMPER
            bundle.tokens[0].gates_q15[0] = 999;
        }

        let json = serde_json::to_string_pretty(&bundle).expect("serialize bundle");
        fs::write(&bundle_path, json).expect("write bundle");
        bundle_path
    }

    #[test]
    fn golden_bundle_passes() {
        let dir = new_test_tempdir();
        let bundle_path = write_bundle(dir.path(), false);

        let report = verify_bundle_from_path(&bundle_path).expect("verification should succeed");
        assert!(
            report.reasons.is_empty(),
            "expected no reasons, got {:?}",
            report.reasons
        );
        assert!(report.context_digest.matches);
        assert!(report.run_head_hash.matches);
        assert!(report.output_digest.matches);
        assert!(report.receipt_digest.matches);
        assert_eq!(report.signature_valid, Some(true));
    }

    #[test]
    fn byte_flip_triggers_trace_tamper() {
        let dir = new_test_tempdir();
        let bundle_path = write_bundle(dir.path(), true);

        let report = verify_bundle_from_path(&bundle_path).expect("verification should run");
        assert!(
            report
                .reasons
                .iter()
                .any(|r| matches!(r, ReasonCode::TraceTamper)),
            "expected TRACE_TAMPER in reasons"
        );
        assert!(!report.run_head_hash.matches);
    }
}
