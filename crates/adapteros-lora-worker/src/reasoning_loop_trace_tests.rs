use super::{ReasoningSwapGuard, MAX_REASONING_SWAPS};
use adapteros_api_types::inference::PolicyOverrideFlags as ApiPolicyOverrides;
use adapteros_core::{AosError, B3Hash};
use adapteros_db::{Db, SqlTraceSink, TraceSink, TraceStart, TraceTokenInput};
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use serial_test::serial;
use sqlx::Row;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
struct TraceRow {
    token_index: u32,
    selected_adapter_ids: Vec<u8>,
    gates_blob: Vec<u8>,
    decision_hash: Vec<u8>,
    policy_mask_digest_b3: Option<Vec<u8>>,
    allowed_mask: Option<Vec<u8>>,
    policy_overrides_json: Option<String>,
    backend_id: Option<String>,
    kernel_version_id: Option<String>,
}

struct LoopRun {
    error: AosError,
    rows: Vec<TraceRow>,
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(ref value) = self.previous {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

#[serial]
#[tokio::test]
async fn verify_reasoning_loop_termination() {
    let _sig_guard = EnvVarGuard::set("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let context_digest = B3Hash::hash(b"reasoning-loop-context").to_bytes();
    let trace_id = "reasoning-loop-trace";

    let first = run_reasoning_loop(trace_id, context_digest).await;
    assert!(
        matches!(&first.error, AosError::ReasoningLoop(msg) if msg.contains(&MAX_REASONING_SWAPS.to_string())),
        "Guard must surface AosError::ReasoningLoop with the swap limit in the message"
    );
    assert_eq!(
        first.rows.len(),
        MAX_REASONING_SWAPS,
        "Trace should capture every swap attempt before the guard trips"
    );
    assert_eq!(
        first.rows.last().map(|row| row.token_index),
        Some((MAX_REASONING_SWAPS as u32) - 1),
        "Last recorded token index should align with the swap limit"
    );

    let second = run_reasoning_loop(trace_id, context_digest).await;
    assert_eq!(
        first.rows, second.rows,
        "Reasoning loop traces must be deterministic across runs"
    );
}

async fn run_reasoning_loop(trace_id: &str, context_digest: [u8; 32]) -> LoopRun {
    let db = Arc::new(Db::new_in_memory().await.expect("db init"));
    let tenant_id = "tenant-reasoning-loop".to_string();
    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind(&tenant_id)
        .bind(&tenant_id)
        .execute(db.pool())
        .await
        .expect("seed tenant");
    let start = TraceStart {
        trace_id: trace_id.to_string(),
        tenant_id: tenant_id.clone(),
        request_id: None,
        context_digest,
        stack_id: None,
        model_id: None,
        policy_id: None,
    };
    let mut sink = SqlTraceSink::new(db.clone(), start, 1)
        .await
        .expect("create trace sink");

    let (mut router, adapter_info, priors, policy_mask) = reasoning_router_fixture();
    let mut guard = ReasoningSwapGuard::new(MAX_REASONING_SWAPS);
    let mut last_adapter: Option<String> = None;

    for idx in 0..=MAX_REASONING_SWAPS {
        let rationale = if idx % 2 == 0 {
            "<thinking>alpha-plan</thinking>"
        } else {
            "<thinking>beta-plan</thinking>"
        };

        let decision = router
            .route_on_reasoning(rationale, &priors, &adapter_info, &policy_mask, None)
            .expect("router decision");

        let adapter_ids_for_trace: Vec<String> = decision
            .indices
            .iter()
            .map(|i| adapter_info[*i as usize].id.clone())
            .collect();
        assert!(
            !adapter_ids_for_trace.is_empty(),
            "Reasoning routing should select at least one adapter at step {}",
            idx
        );
        if let Some(prev) = last_adapter.as_ref() {
            assert_ne!(
                prev, &adapter_ids_for_trace[0],
                "Swap loop should alternate adapters at step {}",
                idx
            );
        }
        last_adapter = Some(adapter_ids_for_trace[0].clone());

        let policy_mask_digest_b3 = decision
            .policy_mask_digest_b3
            .as_ref()
            .map(|digest| digest.to_bytes());
        let policy_overrides: Option<ApiPolicyOverrides> = decision
            .policy_overrides_applied
            .as_ref()
            .map(|flags| ApiPolicyOverrides {
                allow_list: flags.allow_list,
                deny_list: flags.deny_list,
                trust_state: flags.trust_state,
            });

        sink.record_token(TraceTokenInput {
            token_index: idx as u32,
            adapter_ids: adapter_ids_for_trace,
            gates_q15: decision.gates_q15.iter().copied().collect(),
            policy_mask_digest_b3,
            allowed_mask: Some(policy_mask.allowed.clone()),
            policy_overrides_applied: policy_overrides,
            backend_id: Some("deterministic-backend".to_string()),
            kernel_version_id: Some("reasoning-loop-kernel".to_string()),
        })
        .await
        .expect("record trace");

        if let Err(error) = guard.record_swap() {
            sink.flush().await.expect("flush trace sink");
            let rows = load_trace_rows(&db, trace_id).await;

            assert_eq!(
                guard.count(),
                MAX_REASONING_SWAPS,
                "Guard should trip exactly at the configured limit"
            );
            assert_eq!(
                rows.len(),
                MAX_REASONING_SWAPS,
                "TraceDb must have one entry per swap attempt"
            );

            return LoopRun { error, rows };
        }
    }

    panic!("Reasoning swap guard did not trigger");
}

async fn load_trace_rows(db: &Db, trace_id: &str) -> Vec<TraceRow> {
    let rows = sqlx::query(
        r#"
        SELECT
            token_index,
            selected_adapter_ids,
            gates_q15,
            decision_hash,
            policy_mask_digest,
            allowed_mask,
            policy_overrides_json,
            backend_id,
            kernel_version_id
        FROM inference_trace_tokens
        WHERE trace_id = ?
        ORDER BY token_index
        "#,
    )
    .bind(trace_id)
    .fetch_all(db.pool())
    .await
    .expect("load trace rows");

    rows.into_iter()
        .map(|row| TraceRow {
            token_index: row.get::<i64, _>("token_index") as u32,
            selected_adapter_ids: row.get("selected_adapter_ids"),
            gates_blob: row.get("gates_q15"),
            decision_hash: row.get("decision_hash"),
            policy_mask_digest_b3: row.get::<Option<Vec<u8>>, _>("policy_mask_digest"),
            allowed_mask: row.get::<Option<Vec<u8>>, _>("allowed_mask"),
            policy_overrides_json: row.get::<Option<String>, _>("policy_overrides_json"),
            backend_id: row.get::<Option<String>, _>("backend_id"),
            kernel_version_id: row.get::<Option<String>, _>("kernel_version_id"),
        })
        .collect()
}

fn reasoning_router_fixture() -> (Router, Vec<AdapterInfo>, Vec<f32>, PolicyMask) {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    let adapter_info = vec![
        AdapterInfo {
            id: "alpha-adapter".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["alpha".to_string()],
            ..Default::default()
        },
        AdapterInfo {
            id: "beta-adapter".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["beta".to_string()],
            ..Default::default()
        },
    ];
    let priors = vec![0.55, 0.55];
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    (router, adapter_info, priors, policy_mask)
}
