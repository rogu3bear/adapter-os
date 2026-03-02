#![allow(clippy::large_enum_variant)]
#![allow(clippy::too_many_arguments)]

use adapteros_api_types::inference::PolicyOverrideFlags;
use adapteros_core::B3Hash;
use adapteros_db::{Db, SqlTraceSink, TraceFinalization, TraceSink, TraceStart, TraceTokenInput};
use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::Row;
use std::{fs, path::Path, sync::Arc};

#[derive(Debug, Clone, Deserialize)]
struct TokenFixture {
    token_index: u32,
    adapter_ids: Vec<String>,
    gates_q15: Vec<i16>,
    #[serde(default)]
    policy_mask_digest: Option<String>,
    #[serde(default)]
    allowed_mask: Option<Vec<bool>>,
    #[serde(default)]
    policy_overrides_applied: Option<PolicyOverrideFlags>,
    #[serde(default)]
    backend_id: Option<String>,
    #[serde(default)]
    kernel_version_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ExpectedBasic {
    run_head: String,
    output_digest: String,
    receipt_digest: String,
    #[serde(default)]
    merkle_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PolicyAppendExpected {
    base_run_head: String,
    base_output_digest: String,
    base_receipt_digest: String,
    appended_run_head: String,
    appended_output_digest: String,
    appended_receipt_digest: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
enum Fixture {
    #[serde(rename = "inference")]
    Inference {
        name: String,
        reason_code: String,
        context_digest: String,
        tokens: Vec<TokenFixture>,
        output_tokens: Vec<u32>,
        expected: ExpectedBasic,
        #[serde(default = "default_true")]
        assert_output_count_matches_tokens: bool,
    },
    #[serde(rename = "routing")]
    Routing {
        name: String,
        reason_code: String,
        context_digest: String,
        tokens: Vec<TokenFixture>,
        output_tokens: Vec<u32>,
        expected: ExpectedBasic,
        #[serde(default = "default_true")]
        assert_output_count_matches_tokens: bool,
    },
    #[serde(rename = "policy_append")]
    PolicyAppend {
        name: String,
        reason_code: String,
        context_digest: String,
        base_tokens: Vec<TokenFixture>,
        append_token: TokenFixture,
        base_output_tokens: Vec<u32>,
        output_tokens: Vec<u32>,
        expected: PolicyAppendExpected,
    },
}

fn default_true() -> bool {
    true
}

impl TokenFixture {
    fn policy_digest_bytes(&self) -> Result<Option<[u8; 32]>> {
        self.policy_mask_digest
            .as_ref()
            .map(|hex| {
                B3Hash::from_hex(hex)
                    .map(|h| h.to_bytes())
                    .with_context(|| format!("invalid policy_mask_digest hex: {}", hex))
            })
            .transpose()
    }

    fn to_input(&self) -> Result<TraceTokenInput> {
        Ok(TraceTokenInput {
            token_index: self.token_index,
            adapter_ids: self.adapter_ids.clone(),
            gates_q15: self.gates_q15.clone(),
            policy_mask_digest_b3: self.policy_digest_bytes()?,
            allowed_mask: self.allowed_mask.clone(),
            policy_overrides_applied: self.policy_overrides_applied.clone(),
            backend_id: self.backend_id.clone(),
            kernel_version_id: self.kernel_version_id.clone(),
        })
    }
}

fn hex_to_b3(hex: &str) -> Result<B3Hash> {
    B3Hash::from_hex(hex).with_context(|| format!("invalid digest hex: {}", hex))
}

fn hex_to_digest_bytes(hex: &str) -> Result<[u8; 32]> {
    Ok(hex_to_b3(hex)?.to_bytes())
}

fn assert_digest(reason: &str, fixture: &str, label: &str, expected: &B3Hash, actual: &B3Hash) {
    assert_eq!(
        expected.to_hex(),
        actual.to_hex(),
        "[{reason}] drift in {fixture} for {label}\nexpected: {}\nactual:   {}",
        expected.to_hex(),
        actual.to_hex()
    );
}

async fn init_in_memory_db() -> Result<Arc<Db>> {
    // Skip migration signature verification for this smoke test.
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let db = Arc::new(Db::connect(":memory:").await?);
    let pool = db.pool_result().unwrap();

    // Minimal schema needed for trace recording.
    sqlx::query(
        r#"
        CREATE TABLE inference_traces (
            trace_id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            request_id TEXT,
            context_digest BLOB NOT NULL,
            stack_id TEXT,
            model_id TEXT,
            policy_id TEXT,
            status TEXT NOT NULL DEFAULT 'running',
            created_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE inference_trace_tokens (
            trace_id TEXT NOT NULL,
            token_index INTEGER NOT NULL,
            selected_adapter_ids BLOB NOT NULL,
            gates_q15 BLOB NOT NULL,
            decision_hash BLOB NOT NULL,
            policy_mask_digest BLOB,
            allowed_mask BLOB,
            policy_overrides_json TEXT,
            backend_id TEXT,
            kernel_version_id TEXT,
            created_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE inference_trace_receipts (
            trace_id TEXT PRIMARY KEY,
            run_head_hash BLOB NOT NULL,
            output_digest BLOB NOT NULL,
            input_digest_b3 BLOB,
            receipt_digest BLOB NOT NULL,
            logical_prompt_tokens INTEGER NOT NULL,
            prefix_cached_token_count INTEGER NOT NULL,
            billed_input_tokens INTEGER NOT NULL,
            logical_output_tokens INTEGER NOT NULL,
            billed_output_tokens INTEGER NOT NULL,
            signature BLOB,
            attestation BLOB,
            stop_reason_code TEXT,
            stop_reason_token_index INTEGER,
            stop_policy_digest_b3 BLOB,
            -- KV quota/residency fields (PRD: KvResidencyAndQuotas v1)
            tenant_kv_quota_bytes INTEGER NOT NULL DEFAULT 0,
            tenant_kv_bytes_used INTEGER NOT NULL DEFAULT 0,
            kv_evictions INTEGER NOT NULL DEFAULT 0,
            kv_residency_policy_id TEXT,
            kv_quota_enforced INTEGER NOT NULL DEFAULT 0,
            model_cache_identity_v2_digest_b3 BLOB,
            prefix_kv_key_b3 TEXT,
            prefix_cache_hit INTEGER NOT NULL DEFAULT 0,
            prefix_kv_bytes INTEGER NOT NULL DEFAULT 0,
            equipment_profile_digest_b3 BLOB,
            processor_id TEXT,
            mlx_version TEXT,
            ane_version TEXT,
            crypto_receipt_digest_b3 BLOB,
            receipt_parity_verified INTEGER,
            tenant_id TEXT,
            created_at TEXT,
            copy_bytes INTEGER,
            tokenizer_hash_b3 BLOB,
            tokenizer_version TEXT,
            tokenizer_normalization TEXT,
            model_build_hash_b3 BLOB,
            adapter_build_hash_b3 BLOB,
            decode_algo TEXT,
            temperature_q15 INTEGER,
            top_p_q15 INTEGER,
            top_k INTEGER,
            seed_digest_b3 BLOB,
            sampling_backend TEXT,
            thread_count INTEGER,
            reduction_strategy TEXT,
            stop_eos_q15 INTEGER,
            stop_window_digest_b3 BLOB,
            cache_scope TEXT,
            cached_prefix_digest_b3 BLOB,
            cached_prefix_len INTEGER,
            cache_key_b3 BLOB,
            retrieval_merkle_root_b3 BLOB,
            retrieval_order_digest_b3 BLOB,
            tool_call_inputs_digest_b3 BLOB,
            tool_call_outputs_digest_b3 BLOB,
            disclosure_level TEXT,
            receipt_signing_kid TEXT,
            receipt_signed_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(db)
}

async fn run_trace_once(
    trace_label: &str,
    context_digest: [u8; 32],
    tokens: &[TokenFixture],
    output_tokens: &[u32],
) -> Result<(adapteros_db::TraceReceipt, Arc<Db>, usize)> {
    let db = init_in_memory_db().await?;
    let start = TraceStart {
        trace_id: format!("trace-{trace_label}"),
        tenant_id: "tenant-smoke".to_string(),
        request_id: Some(trace_label.to_string()),
        context_digest,
        stack_id: None,
        model_id: None,
        policy_id: None,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 8).await?;
    for token in tokens {
        sink.record_token(token.to_input()?).await?;
    }
    let logical_prompt_tokens = output_tokens.len() as u32;
    let logical_output_tokens = output_tokens.len() as u32;
    let finalization = TraceFinalization {
        output_tokens,
        logical_prompt_tokens,
        prefix_cached_token_count: 0,
        billed_input_tokens: logical_prompt_tokens,
        logical_output_tokens,
        billed_output_tokens: logical_output_tokens,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
        // P0-1: Cache attestation (not needed when prefix_cached_token_count = 0)
        cache_attestation: None,
        worker_public_key: None,
        // UMA telemetry (PRD §5.5)
        copy_bytes: None,
        // V7 fields
        tokenizer_hash_b3: None,
        tokenizer_version: None,
        tokenizer_normalization: None,
        model_build_hash_b3: None,
        adapter_build_hash_b3: None,
        decode_algo: None,
        temperature_q15: None,
        top_p_q15: None,
        top_k: None,
        seed_digest_b3: None,
        sampling_backend: None,
        thread_count: None,
        reduction_strategy: None,
        stop_eos_q15: None,
        stop_window_digest_b3: None,
        cache_scope: None,
        cached_prefix_digest_b3: None,
        cached_prefix_len: None,
        cache_key_b3: None,
        retrieval_merkle_root_b3: None,
        retrieval_order_digest_b3: None,
        tool_call_inputs_digest_b3: None,
        tool_call_outputs_digest_b3: None,
        disclosure_level: None,
        receipt_signing_kid: None,
        receipt_signed_at: None,
    };

    let receipt = sink.finalize(finalization).await?;

    let token_count: i64 =
        sqlx::query("SELECT COUNT(*) AS cnt FROM inference_trace_tokens WHERE trace_id = ?")
            .bind(&receipt.trace_id)
            .fetch_one(db.pool_result().unwrap())
            .await?
            .get("cnt");

    Ok((receipt, db, token_count as usize))
}

async fn verify_basic_fixture(
    name: &str,
    reason_code: &str,
    context_hex: &str,
    tokens: &[TokenFixture],
    output_tokens: &[u32],
    expected: &ExpectedBasic,
    assert_count_match: bool,
) -> Result<()> {
    let context_digest = hex_to_digest_bytes(context_hex)?;

    // Two independent runs to prove repeatability.
    let (first_receipt, _, first_count) =
        run_trace_once(&format!("{name}-a"), context_digest, tokens, output_tokens).await?;
    let (second_receipt, _, second_count) =
        run_trace_once(&format!("{name}-b"), context_digest, tokens, output_tokens).await?;

    let expected_run_head = hex_to_b3(&expected.run_head)?;
    let expected_output = hex_to_b3(&expected.output_digest)?;
    let expected_receipt = hex_to_b3(&expected.receipt_digest)?;

    assert_digest(
        reason_code,
        name,
        "run_head (expected)",
        &expected_run_head,
        &first_receipt.run_head_hash,
    );
    assert_digest(
        reason_code,
        name,
        "output_digest (expected)",
        &expected_output,
        &first_receipt.output_digest,
    );
    assert_digest(
        reason_code,
        name,
        "receipt_digest (expected)",
        &expected_receipt,
        &first_receipt.receipt_digest,
    );

    if let Some(root_hex) = &expected.merkle_root {
        let merkle_expected = hex_to_b3(root_hex)?;
        assert_digest(
            reason_code,
            name,
            "merkle_root",
            &merkle_expected,
            &first_receipt.run_head_hash,
        );
    }

    // Repeatability checks
    assert_digest(
        reason_code,
        name,
        "run_head (repeat)",
        &first_receipt.run_head_hash,
        &second_receipt.run_head_hash,
    );
    assert_digest(
        reason_code,
        name,
        "output_digest (repeat)",
        &first_receipt.output_digest,
        &second_receipt.output_digest,
    );
    assert_digest(
        reason_code,
        name,
        "receipt_digest (repeat)",
        &first_receipt.receipt_digest,
        &second_receipt.receipt_digest,
    );

    if assert_count_match {
        assert_eq!(
            first_count,
            output_tokens.len(),
            "[{reason_code}] {name}: token count mismatch vs output length\nexpected tokens: {}\nactual tokens: {}",
            output_tokens.len(),
            first_count
        );
        assert_eq!(
            second_count, first_count,
            "[{reason_code}] {name}: token count drift between runs\nrun_a: {first_count}\nrun_b: {second_count}"
        );
    }

    Ok(())
}

async fn verify_policy_append_fixture(
    name: &str,
    reason_code: &str,
    context_hex: &str,
    base_tokens: &[TokenFixture],
    append_token: &TokenFixture,
    base_output_tokens: &[u32],
    output_tokens: &[u32],
    expected: &PolicyAppendExpected,
) -> Result<()> {
    let context_digest = hex_to_digest_bytes(context_hex)?;
    let mut appended_tokens = base_tokens.to_vec();
    appended_tokens.push(append_token.clone());

    // Base chain (no append)
    let (base_receipt_a, _, base_count_a) = run_trace_once(
        &format!("{name}-base-a"),
        context_digest,
        base_tokens,
        base_output_tokens,
    )
    .await?;
    let (base_receipt_b, _, base_count_b) = run_trace_once(
        &format!("{name}-base-b"),
        context_digest,
        base_tokens,
        base_output_tokens,
    )
    .await?;

    let expected_base_run_head = hex_to_b3(&expected.base_run_head)?;
    let expected_base_output = hex_to_b3(&expected.base_output_digest)?;
    let expected_base_receipt = hex_to_b3(&expected.base_receipt_digest)?;

    assert_digest(
        reason_code,
        name,
        "base run_head",
        &expected_base_run_head,
        &base_receipt_a.run_head_hash,
    );
    assert_digest(
        reason_code,
        name,
        "base output_digest",
        &expected_base_output,
        &base_receipt_a.output_digest,
    );
    assert_digest(
        reason_code,
        name,
        "base receipt_digest",
        &expected_base_receipt,
        &base_receipt_a.receipt_digest,
    );
    assert_digest(
        reason_code,
        name,
        "base run_head repeat",
        &base_receipt_a.run_head_hash,
        &base_receipt_b.run_head_hash,
    );

    assert_eq!(
        base_count_a,
        base_output_tokens.len(),
        "[{reason_code}] {name}: base token count mismatch\nexpected tokens: {}\nactual tokens: {}",
        base_output_tokens.len(),
        base_count_a
    );
    assert_eq!(
        base_count_b, base_count_a,
        "[{reason_code}] {name}: base token count drift between runs\nrun_a: {base_count_a}\nrun_b: {base_count_b}"
    );

    // Appended chain
    let (appended_receipt_a, _, appended_count_a) = run_trace_once(
        &format!("{name}-appended-a"),
        context_digest,
        &appended_tokens,
        output_tokens,
    )
    .await?;
    let (appended_receipt_b, _, appended_count_b) = run_trace_once(
        &format!("{name}-appended-b"),
        context_digest,
        &appended_tokens,
        output_tokens,
    )
    .await?;

    let expected_app_run_head = hex_to_b3(&expected.appended_run_head)?;
    let expected_app_output = hex_to_b3(&expected.appended_output_digest)?;
    let expected_app_receipt = hex_to_b3(&expected.appended_receipt_digest)?;

    assert_digest(
        reason_code,
        name,
        "appended run_head",
        &expected_app_run_head,
        &appended_receipt_a.run_head_hash,
    );
    assert_digest(
        reason_code,
        name,
        "appended output_digest",
        &expected_app_output,
        &appended_receipt_a.output_digest,
    );
    assert_digest(
        reason_code,
        name,
        "appended receipt_digest",
        &expected_app_receipt,
        &appended_receipt_a.receipt_digest,
    );

    assert_digest(
        reason_code,
        name,
        "appended run_head repeat",
        &appended_receipt_a.run_head_hash,
        &appended_receipt_b.run_head_hash,
    );
    assert_digest(
        reason_code,
        name,
        "appended receipt repeat",
        &appended_receipt_a.receipt_digest,
        &appended_receipt_b.receipt_digest,
    );

    assert_eq!(
        appended_count_a,
        output_tokens.len(),
        "[{reason_code}] {name}: appended token count mismatch\nexpected tokens: {}\nactual tokens: {}",
        output_tokens.len(),
        appended_count_a
    );
    assert_eq!(
        appended_count_b, appended_count_a,
        "[{reason_code}] {name}: appended token count drift between runs\nrun_a: {appended_count_a}\nrun_b: {appended_count_b}"
    );

    // Ensure append changes the Merkle root compared to base but remains predictable.
    assert_ne!(
        base_receipt_a.run_head_hash.to_hex(),
        appended_receipt_a.run_head_hash.to_hex(),
        "[{reason_code}] {name}: append should change run_head"
    );

    Ok(())
}

#[tokio::test]
async fn determinism_smoke_fixtures_hold() -> Result<()> {
    let dir = Path::new("tests/golden");
    let mut fixtures = Vec::new();

    for entry in fs::read_dir(dir).context("golden fixture directory missing")? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let data = fs::read_to_string(entry.path())?;
        let fixture: Fixture = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse fixture {:?}", entry.path()))?;
        fixtures.push(fixture);
    }

    assert!(
        !fixtures.is_empty(),
        "No golden fixtures found in tests/golden"
    );

    for fixture in fixtures {
        match fixture {
            Fixture::Inference {
                name,
                reason_code,
                context_digest,
                tokens,
                output_tokens,
                expected,
                assert_output_count_matches_tokens,
            }
            | Fixture::Routing {
                name,
                reason_code,
                context_digest,
                tokens,
                output_tokens,
                expected,
                assert_output_count_matches_tokens,
            } => {
                verify_basic_fixture(
                    &name,
                    &reason_code,
                    &context_digest,
                    &tokens,
                    &output_tokens,
                    &expected,
                    assert_output_count_matches_tokens,
                )
                .await?;
            }
            Fixture::PolicyAppend {
                name,
                reason_code,
                context_digest,
                base_tokens,
                append_token,
                base_output_tokens,
                output_tokens,
                expected,
            } => {
                verify_policy_append_fixture(
                    &name,
                    &reason_code,
                    &context_digest,
                    &base_tokens,
                    &append_token,
                    &base_output_tokens,
                    &output_tokens,
                    &expected,
                )
                .await?;
            }
        }
    }

    Ok(())
}
