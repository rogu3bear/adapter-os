//! Shared receipt verification implementation.
//!
//! Goal: server and CLI must not drift. Any verification behavior should live
//! here and be reused by both `adapteros-server-api` and `aosctl`.
//!
//! This module intentionally uses `adapteros_core::receipt_digest` for digest
//! computation to preserve production parity.

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use std::io::{Cursor, Read};
use zip::ZipArchive;

use adapteros_core::receipt_digest::{
    self, ReceiptDigestInput, RECEIPT_SCHEMA_V1, RECEIPT_SCHEMA_V2, RECEIPT_SCHEMA_V3,
    RECEIPT_SCHEMA_V4, RECEIPT_SCHEMA_V5, RECEIPT_SCHEMA_V6, RECEIPT_SCHEMA_V7,
};
use adapteros_core::B3Hash;

use crate::signature::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

const EVIDENCE_BUNDLE_FILENAMES: &[&str] = &[
    "receipt_bundle.json",
    "run_receipt.json",
    "inference_trace.json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReasonCode {
    ContextMismatch,
    TraceTamper,
    OutputMismatch,
    PolicyMismatch,
    BackendMismatch,
    SignatureInvalid,
    BackendAttestationMismatch,
    SchemaVersionUnsupported,
    SeedDigestMismatch,
    SeedModeViolation,
    SeedDigestMissing,
    // Receipt payload verification reasons
    ExpectedDigestInvalid,
    PayloadParseError,
    NonCanonicalPayload,
    ReceiptDigestMismatch,
}

impl ReasonCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContextMismatch => "CONTEXT_MISMATCH",
            Self::TraceTamper => "TRACE_TAMPER",
            Self::OutputMismatch => "OUTPUT_MISMATCH",
            Self::PolicyMismatch => "POLICY_MISMATCH",
            Self::BackendMismatch => "BACKEND_MISMATCH",
            Self::SignatureInvalid => "SIGNATURE_INVALID",
            Self::BackendAttestationMismatch => "BACKEND_ATTESTATION_MISMATCH",
            Self::SchemaVersionUnsupported => "SCHEMA_VERSION_UNSUPPORTED",
            Self::SeedDigestMismatch => "SEED_DIGEST_MISMATCH",
            Self::SeedModeViolation => "SEED_MODE_VIOLATION",
            Self::SeedDigestMissing => "SEED_DIGEST_MISSING",
            Self::ExpectedDigestInvalid => "EXPECTED_DIGEST_INVALID",
            Self::PayloadParseError => "PAYLOAD_PARSE_ERROR",
            Self::NonCanonicalPayload => "NON_CANONICAL_PAYLOAD",
            Self::ReceiptDigestMismatch => "RECEIPT_DIGEST_MISMATCH",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DigestComparison {
    pub computed: String,
    pub expected: String,
    pub matches: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReceiptVerificationReport {
    pub trace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub source: String,
    pub pass: bool,
    pub verified_at: String,
    pub reasons: Vec<ReasonCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mismatched_token: Option<u32>,
    pub context_digest: DigestComparison,
    pub run_head_hash: DigestComparison,
    pub output_digest: DigestComparison,
    pub receipt_digest: DigestComparison,
    pub signature_checked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_valid: Option<bool>,
}

#[derive(Debug, Default, Clone)]
pub struct VerifyOptions {
    /// Expected seed for digest verification (32 bytes)
    pub expected_seed: Option<[u8; 32]>,
    /// Require seed digest in receipt (fail if missing)
    pub require_seed_digest: bool,
    /// Expected seed mode (e.g., "strict", "best_effort")
    pub expected_seed_mode: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReceiptPayloadVerifyResult {
    pub pass: bool,
    pub reasons: Vec<ReasonCode>,
    pub schema_version: u8,
    pub expected_receipt_digest_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed_receipt_digest_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
}

fn push_reason(reasons: &mut Vec<ReasonCode>, code: ReasonCode) {
    if !reasons.contains(&code) {
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

    // NOTE: This is the historical evidence-bundle decision framing used by
    // existing exported bundles (and by legacy server/CLI verifiers). Do not
    // change lightly.
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

fn compute_context_digest(ctx: &ReceiptContext) -> Result<B3Hash> {
    let stack_bytes =
        hex::decode(&ctx.stack_hash_hex).with_context(|| "Failed to decode stack_hash_hex")?;
    let mut buf = Vec::with_capacity(
        ctx.tenant_namespace.len() + stack_bytes.len() + 4 + ctx.prompt_tokens.len() * 4 + 96,
    );
    buf.extend_from_slice(ctx.tenant_namespace.as_bytes());
    buf.extend_from_slice(&stack_bytes);
    if let Some(ref hex) = ctx.tokenizer_hash_b3_hex {
        if let Ok(bytes) = hex::decode(hex) {
            buf.extend_from_slice(&bytes);
        }
        if let Some(ref version) = ctx.tokenizer_version {
            buf.extend_from_slice(&(version.len() as u32).to_le_bytes());
            buf.extend_from_slice(version.as_bytes());
        }
        if let Some(ref norm) = ctx.tokenizer_normalization {
            buf.extend_from_slice(&(norm.len() as u32).to_le_bytes());
            buf.extend_from_slice(norm.as_bytes());
        }
    }
    buf.extend_from_slice(&(ctx.prompt_tokens.len() as u32).to_le_bytes());
    for t in &ctx.prompt_tokens {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    Ok(B3Hash::hash(&buf))
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

fn verify_seed_binding(
    bundle: &ReceiptBundle,
    expected_seed: Option<&[u8; 32]>,
) -> Option<ReasonCode> {
    let expected = expected_seed?;
    let Some(ref claimed_digest) = bundle.receipt.root_seed_digest_hex else {
        return None;
    };

    let expected_digest = B3Hash::hash(expected);
    if &expected_digest.to_hex() != claimed_digest {
        Some(ReasonCode::SeedDigestMismatch)
    } else {
        None
    }
}

fn compute_receipt_digest_from_bundle(
    context_digest: &B3Hash,
    run_head: &B3Hash,
    output_digest: &B3Hash,
    receipt: &ReceiptDigests,
    bundle: &ReceiptBundle,
) -> B3Hash {
    let mut input = ReceiptDigestInput::new(
        *context_digest.as_bytes(),
        *run_head.as_bytes(),
        *output_digest.as_bytes(),
        receipt.logical_prompt_tokens,
        receipt.prefix_cached_token_count,
        receipt.billed_input_tokens,
        receipt.logical_output_tokens,
        receipt.billed_output_tokens,
    );

    if receipt.schema_version >= RECEIPT_SCHEMA_V2 {
        let attestation_bytes = bundle
            .backend_attestation_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_backend(bundle.backend_used.clone(), attestation_bytes);
    }

    if receipt.schema_version >= RECEIPT_SCHEMA_V3 {
        let seed_digest = receipt
            .root_seed_digest_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_seed_lineage(
            seed_digest,
            receipt.seed_mode.clone(),
            receipt.has_manifest_binding,
        );
    }

    if receipt.schema_version >= RECEIPT_SCHEMA_V4 {
        let stop_policy_digest = receipt
            .stop_policy_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());

        input = input.with_stop_controller(
            receipt.stop_reason_code.clone(),
            receipt.stop_reason_token_index,
            stop_policy_digest,
        );

        input = input.with_kv_quota(
            receipt.tenant_kv_quota_bytes,
            receipt.tenant_kv_bytes_used,
            receipt.kv_evictions,
            receipt.kv_residency_policy_id.clone(),
            receipt.kv_quota_enforced,
        );

        let prefix_kv_key = receipt
            .prefix_kv_key_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_prefix_cache(
            prefix_kv_key,
            receipt.prefix_cache_hit,
            receipt.prefix_kv_bytes,
        );

        let model_cache_identity = receipt
            .model_cache_identity_v2_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_model_cache_identity(model_cache_identity);
    }

    if receipt.schema_version >= RECEIPT_SCHEMA_V5 {
        let equipment_profile = receipt
            .equipment_profile_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_equipment_profile(
            equipment_profile,
            receipt.processor_id.clone(),
            receipt.mlx_version.clone(),
            receipt.ane_version.clone(),
        );

        let citations_root = receipt
            .citations_merkle_root_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_citations(citations_root, receipt.citation_count);
    }

    if receipt.schema_version >= RECEIPT_SCHEMA_V6 {
        let previous = receipt
            .previous_receipt_digest_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_cross_run_lineage(previous, receipt.session_sequence);
    }

    if receipt.schema_version >= RECEIPT_SCHEMA_V7 {
        let tokenizer_hash = receipt
            .tokenizer_hash_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_tokenizer_identity(
            tokenizer_hash,
            receipt.tokenizer_version.clone(),
            receipt.tokenizer_normalization.clone(),
        );

        let model_build = receipt
            .model_build_hash_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        let adapter_build = receipt
            .adapter_build_hash_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_build_provenance(model_build, adapter_build);

        let seed_digest = receipt
            .seed_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_decoder_config(
            receipt.decode_algo.clone(),
            receipt.temperature_q15,
            receipt.top_p_q15,
            receipt.top_k,
            seed_digest,
            receipt.sampling_backend.clone(),
        );

        input = input
            .with_concurrency_determinism(receipt.thread_count, receipt.reduction_strategy.clone());

        let stop_window = receipt
            .stop_window_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_stop_controller_inputs(receipt.stop_eos_q15, stop_window);

        let cached_prefix = receipt
            .cached_prefix_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        let cache_key = receipt
            .cache_key_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_cache_proof(
            receipt.cache_scope.clone(),
            cached_prefix,
            receipt.cached_prefix_len,
            cache_key,
        );

        let retrieval_merkle = receipt
            .retrieval_merkle_root_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        let retrieval_order = receipt
            .retrieval_order_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        let tool_inputs = receipt
            .tool_call_inputs_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        let tool_outputs = receipt
            .tool_call_outputs_digest_b3_hex
            .as_ref()
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| <[u8; 32]>::try_from(b).ok());
        input = input.with_retrieval_tool_binding(
            retrieval_merkle,
            retrieval_order,
            tool_inputs,
            tool_outputs,
        );

        input = input.with_disclosure_level(receipt.disclosure_level.clone());
    }

    receipt_digest::compute_receipt_digest(&input, receipt.schema_version)
        .unwrap_or_else(B3Hash::zero)
}

fn default_schema_version() -> u8 {
    RECEIPT_SCHEMA_V1
}

#[derive(Debug, Serialize, Deserialize)]
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
    #[serde(default)]
    backend_used: Option<String>,
    #[serde(default)]
    backend_attestation_b3_hex: Option<String>,
    #[serde(default)]
    dataset_version_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReceiptContext {
    tenant_namespace: String,
    stack_hash_hex: String,
    prompt_tokens: Vec<u32>,
    #[serde(default)]
    policy_mask_digest_hex: Option<String>,
    #[serde(default)]
    context_digest_hex: Option<String>,
    #[serde(default)]
    tokenizer_hash_b3_hex: Option<String>,
    #[serde(default)]
    tokenizer_version: Option<String>,
    #[serde(default)]
    tokenizer_normalization: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
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
    #[serde(default = "default_schema_version")]
    schema_version: u8,
    #[serde(default)]
    backend_used: Option<String>,
    #[serde(default)]
    backend_attestation_b3_hex: Option<String>,
    #[serde(default)]
    root_seed_digest_hex: Option<String>,
    #[serde(default)]
    seed_mode: Option<String>,
    #[serde(default)]
    has_manifest_binding: Option<bool>,
    #[serde(default)]
    stop_reason_code: Option<String>,
    #[serde(default)]
    stop_reason_token_index: Option<u32>,
    #[serde(default)]
    stop_policy_digest_b3_hex: Option<String>,
    #[serde(default)]
    tenant_kv_quota_bytes: u64,
    #[serde(default)]
    tenant_kv_bytes_used: u64,
    #[serde(default)]
    kv_evictions: u32,
    #[serde(default)]
    kv_residency_policy_id: Option<String>,
    #[serde(default)]
    kv_quota_enforced: bool,
    #[serde(default)]
    prefix_kv_key_b3_hex: Option<String>,
    #[serde(default)]
    prefix_cache_hit: bool,
    #[serde(default)]
    prefix_kv_bytes: u64,
    #[serde(default)]
    model_cache_identity_v2_digest_b3_hex: Option<String>,
    #[serde(default)]
    equipment_profile_digest_b3_hex: Option<String>,
    #[serde(default)]
    processor_id: Option<String>,
    #[serde(default)]
    mlx_version: Option<String>,
    #[serde(default)]
    ane_version: Option<String>,
    #[serde(default)]
    citations_merkle_root_b3_hex: Option<String>,
    #[serde(default)]
    citation_count: u32,
    #[serde(default)]
    previous_receipt_digest_hex: Option<String>,
    #[serde(default)]
    session_sequence: u64,
    #[serde(default)]
    tokenizer_hash_b3_hex: Option<String>,
    #[serde(default)]
    tokenizer_version: Option<String>,
    #[serde(default)]
    tokenizer_normalization: Option<String>,
    #[serde(default)]
    model_build_hash_b3_hex: Option<String>,
    #[serde(default)]
    adapter_build_hash_b3_hex: Option<String>,
    #[serde(default)]
    decode_algo: Option<String>,
    #[serde(default)]
    temperature_q15: Option<i16>,
    #[serde(default)]
    top_p_q15: Option<i16>,
    #[serde(default)]
    top_k: Option<u32>,
    #[serde(default)]
    seed_digest_b3_hex: Option<String>,
    #[serde(default)]
    sampling_backend: Option<String>,
    #[serde(default)]
    thread_count: Option<u32>,
    #[serde(default)]
    reduction_strategy: Option<String>,
    #[serde(default)]
    stop_eos_q15: Option<i16>,
    #[serde(default)]
    stop_window_digest_b3_hex: Option<String>,
    #[serde(default)]
    cache_scope: Option<String>,
    #[serde(default)]
    cached_prefix_digest_b3_hex: Option<String>,
    #[serde(default)]
    cached_prefix_len: Option<u32>,
    #[serde(default)]
    cache_key_b3_hex: Option<String>,
    #[serde(default)]
    retrieval_merkle_root_b3_hex: Option<String>,
    #[serde(default)]
    retrieval_order_digest_b3_hex: Option<String>,
    #[serde(default)]
    tool_call_inputs_digest_b3_hex: Option<String>,
    #[serde(default)]
    tool_call_outputs_digest_b3_hex: Option<String>,
    #[serde(default)]
    disclosure_level: Option<String>,
    #[serde(default)]
    receipt_signing_kid: Option<String>,
    #[serde(default)]
    receipt_signed_at: Option<String>,
}

fn load_bundle_from_bytes(bytes: &[u8]) -> Result<ReceiptBundle> {
    fn trim_start_ascii_ws(bytes: &[u8]) -> &[u8] {
        let mut i = 0usize;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        &bytes[i..]
    }

    fn looks_like_zip(bytes: &[u8]) -> bool {
        // ZIP signatures: local file header / empty archive / spanned archive.
        bytes.len() >= 4
            && bytes[0] == b'P'
            && bytes[1] == b'K'
            && matches!(
                (bytes[2], bytes[3]),
                (0x03, 0x04) | (0x05, 0x06) | (0x07, 0x08)
            )
    }

    let trimmed = trim_start_ascii_ws(bytes);

    // Avoid "trying zip" on obviously non-zip inputs; this improves error messages and
    // prevents confusing "invalid zip" results for malformed JSON files.
    if !looks_like_zip(trimmed) {
        // Fast path: ReceiptBundle at root.
        if let Ok(bundle) = serde_json::from_slice::<ReceiptBundle>(bytes) {
            return Ok(bundle);
        }

        // Back-compat path: server/client may wrap the bundle under a top-level key.
        let value: serde_json::Value =
            serde_json::from_slice(bytes).with_context(|| "Invalid JSON evidence bundle")?;
        if let Some(nested) = value
            .get("receipt_bundle")
            .or_else(|| value.get("bundle"))
            .cloned()
        {
            return serde_json::from_value::<ReceiptBundle>(nested)
                .with_context(|| "Invalid JSON evidence bundle");
        }

        bail!("Invalid JSON evidence bundle");
    }

    let mut cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(&mut cursor).with_context(|| "Invalid zip evidence bundle")?;

    for name in EVIDENCE_BUNDLE_FILENAMES {
        if let Ok(mut file) = archive.by_name(name) {
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            if let Ok(bundle) = serde_json::from_str::<ReceiptBundle>(&buf) {
                return Ok(bundle);
            }
        }
    }

    bail!("Zip evidence bundle did not contain a supported receipt JSON file");
}

fn verify_bundle_inner(
    bundle: &ReceiptBundle,
    options: &VerifyOptions,
) -> Result<ReceiptVerificationReport> {
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
        let adapter_blob = receipt_digest::encode_adapter_ids(&token.adapter_ids);
        let gates_blob = receipt_digest::encode_gates_q15(&token.gates_q15);
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

    let output_digest = receipt_digest::compute_output_digest(&bundle.output_tokens);
    let expected_output = B3Hash::from_hex(&bundle.receipt.output_digest_hex)
        .with_context(|| "Invalid output_digest_hex")?;
    if expected_output != output_digest {
        push_reason(&mut reasons, ReasonCode::OutputMismatch);
    }

    let receipt_digest = compute_receipt_digest_from_bundle(
        &computed_context,
        &run_head,
        &output_digest,
        &bundle.receipt,
        bundle,
    );
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

    // Seed options checks (CLI-only; server passes defaults)
    if let Some(reason) = verify_seed_binding(bundle, options.expected_seed.as_ref()) {
        push_reason(&mut reasons, reason);
    }
    if options.require_seed_digest && bundle.receipt.root_seed_digest_hex.is_none() {
        push_reason(&mut reasons, ReasonCode::SeedDigestMissing);
    }
    if let Some(ref expected_mode) = options.expected_seed_mode {
        if let Some(ref actual_mode) = bundle.receipt.seed_mode {
            if actual_mode.to_lowercase() != expected_mode.to_lowercase() {
                push_reason(&mut reasons, ReasonCode::SeedModeViolation);
            }
        } else if bundle.receipt.schema_version >= RECEIPT_SCHEMA_V3 {
            push_reason(&mut reasons, ReasonCode::SeedModeViolation);
        }
    }

    let (signature_checked, signature_valid) =
        verify_signature(&bundle.receipt, &expected_receipt, &mut reasons)?;

    let pass = reasons.is_empty();

    Ok(ReceiptVerificationReport {
        trace_id: bundle.trace_id.clone(),
        tenant_id: Some(bundle.tenant_id.clone()),
        source: "bundle".to_string(),
        pass,
        verified_at: chrono::Utc::now().to_rfc3339(),
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
    })
}

/// Verify an evidence bundle payload (JSON or ZIP bytes).
pub fn verify_bundle_bytes(
    bytes: &[u8],
    options: &VerifyOptions,
) -> Result<ReceiptVerificationReport> {
    let bundle = load_bundle_from_bytes(bytes)?;
    verify_bundle_inner(&bundle, options)
}

/// Verify a receipt digest payload (canonical JSON of `ReceiptDigestInput`) against an expected digest.
///
/// This is the lowest-level, third-party reimplementable verifier used by golden vectors.
pub fn verify_receipt_payload_bytes(
    payload_bytes: &[u8],
    expected_digest_hex: &str,
    schema_version: u8,
) -> ReceiptPayloadVerifyResult {
    let mut reasons: Vec<ReasonCode> = Vec::new();

    let expected = match B3Hash::from_hex(expected_digest_hex.trim()) {
        Ok(h) => h,
        Err(e) => {
            push_reason(&mut reasons, ReasonCode::ExpectedDigestInvalid);
            return ReceiptPayloadVerifyResult {
                pass: false,
                reasons,
                schema_version,
                expected_receipt_digest_hex: expected_digest_hex.to_string(),
                computed_receipt_digest_hex: None,
                canonical_json: None,
                parse_error: Some(e.to_string()),
            };
        }
    };

    let input: ReceiptDigestInput = match serde_json::from_slice(payload_bytes) {
        Ok(v) => v,
        Err(e) => {
            push_reason(&mut reasons, ReasonCode::PayloadParseError);
            return ReceiptPayloadVerifyResult {
                pass: false,
                reasons,
                schema_version,
                expected_receipt_digest_hex: expected_digest_hex.to_string(),
                computed_receipt_digest_hex: None,
                canonical_json: None,
                parse_error: Some(e.to_string()),
            };
        }
    };

    let canonical_json = receipt_digest::canonical_json_string(&input)
        .ok()
        .unwrap_or_else(|| String::new());
    let canonical_bytes = canonical_json.as_bytes();
    let original_trimmed = payload_bytes.strip_suffix(b"\n").unwrap_or(payload_bytes);
    if original_trimmed != canonical_bytes {
        push_reason(&mut reasons, ReasonCode::NonCanonicalPayload);
    }

    let computed = match receipt_digest::compute_receipt_digest(&input, schema_version) {
        Some(d) => d,
        None => {
            push_reason(&mut reasons, ReasonCode::SchemaVersionUnsupported);
            return ReceiptPayloadVerifyResult {
                pass: false,
                reasons,
                schema_version,
                expected_receipt_digest_hex: expected_digest_hex.to_string(),
                computed_receipt_digest_hex: None,
                canonical_json: Some(canonical_json),
                parse_error: None,
            };
        }
    };

    if computed != expected {
        push_reason(&mut reasons, ReasonCode::ReceiptDigestMismatch);
    }

    ReceiptPayloadVerifyResult {
        pass: reasons.is_empty(),
        reasons,
        schema_version,
        expected_receipt_digest_hex: expected.to_hex(),
        computed_receipt_digest_hex: Some(computed.to_hex()),
        canonical_json: Some(canonical_json),
        parse_error: None,
    }
}
