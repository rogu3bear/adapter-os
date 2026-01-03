use crate::types::{InferenceError, InferenceRequestInternal, SamplingParams};
use adapteros_core::determinism::{expand_u64_seed, DeterminismSource};
use adapteros_core::{derive_request_seed, B3Hash, SeedMode};
use adapteros_db::InferenceReplayMetadata;
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use tracing::warn;

// Re-export for downstream callers to avoid churn
pub use adapteros_core::determinism::DeterminismContext;

/// Build determinism context from an in-flight request.
pub fn from_request(
    request: &InferenceRequestInternal,
    manifest_hash: Option<&B3Hash>,
    global_seed: &B3Hash,
    seed_mode: SeedMode,
    worker_id: u32,
) -> Result<DeterminismContext, InferenceError> {
    let request_seed = if let Some(seed) = request.request_seed {
        seed
    } else {
        derive_request_seed(
            global_seed,
            manifest_hash,
            &request.cpid,
            &request.request_id,
            worker_id,
            0,
            seed_mode,
        )
        .map_err(|e| {
            InferenceError::ValidationError(format!("Failed to derive request seed: {}", e))
        })?
    };

    let routing_mode = request
        .routing_determinism_mode
        .unwrap_or(RoutingDeterminismMode::Deterministic);

    Ok(DeterminismContext::new(
        request_seed,
        manifest_hash,
        seed_mode,
        routing_mode,
        DeterminismSource::DerivedFromRequest,
    ))
}

/// Build determinism context from persisted replay metadata.
pub fn from_replay_metadata(
    metadata: &InferenceReplayMetadata,
) -> Result<DeterminismContext, InferenceError> {
    let sampling_params: SamplingParams = serde_json::from_str(&metadata.sampling_params_json)
        .map_err(|e| {
            InferenceError::ValidationError(format!(
                "Failed to parse sampling params from replay metadata: {}",
                e
            ))
        })?;

    let (request_seed, source) = if let Some(hex_seed) = sampling_params.request_seed_hex.clone() {
        let bytes = hex::decode(hex_seed).map_err(|e| {
            InferenceError::ValidationError(format!(
                "Invalid request_seed_hex in replay metadata: {}",
                e
            ))
        })?;
        let seed_bytes: [u8; 32] = bytes.try_into().map_err(|_| {
            InferenceError::ValidationError("request_seed_hex must be 32 bytes".to_string())
        })?;
        (seed_bytes, DeterminismSource::RequestSeedHex)
    } else if let Some(seed64) = sampling_params.seed {
        (expand_u64_seed(seed64), DeterminismSource::SeedU64Expanded)
    } else {
        warn!(
            inference_id = %metadata.inference_id,
            "Replay metadata missing determinism seed; rejecting legacy/seedless replay"
        );
        return Err(InferenceError::ValidationError(
            "Replay metadata missing request_seed; legacy seed derivation is no longer supported—please re-record the replay"
                .to_string(),
        ));
    };

    let manifest_hash = B3Hash::from_hex(&metadata.manifest_hash).ok();
    let seed_mode = sampling_params.seed_mode.unwrap_or(SeedMode::BestEffort);
    let routing_mode = RoutingDeterminismMode::Deterministic;

    Ok(DeterminismContext::new_with_router_seed(
        request_seed,
        metadata.router_seed.clone(),
        manifest_hash.as_ref(),
        seed_mode,
        routing_mode,
        source,
    ))
}

#[cfg(test)]
mod tests {
    use super::{from_replay_metadata, from_request};
    use crate::types::{InferenceRequestInternal, SamplingParams};
    use adapteros_core::determinism::DeterminismSource;
    use adapteros_core::{B3Hash, SeedMode};
    use adapteros_db::InferenceReplayMetadata;
    use adapteros_types::adapters::metadata::RoutingDeterminismMode;

    #[test]
    fn replay_round_trip_preserves_seeds() {
        let manifest = B3Hash::hash(b"manifest");
        let global = B3Hash::hash(b"global");

        let mut request =
            InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        request.request_id = "req-123".to_string();

        let ctx_from_request =
            from_request(&request, Some(&manifest), &global, SeedMode::BestEffort, 7)
                .expect("request context should derive");

        let sampling_params = SamplingParams {
            temperature: 0.0,
            top_k: Some(4),
            top_p: Some(0.9),
            max_tokens: 16,
            seed: Some(ctx_from_request.request_seed_low64()),
            error_code: None,
            seed_mode: Some(SeedMode::BestEffort),
            backend_profile: None,
            request_seed_hex: Some(hex::encode(ctx_from_request.request_seed())),
            placement: None,
            run_envelope: None,
            adapter_hashes_b3: None,
            dataset_hash_b3: None,
        };

        let metadata = InferenceReplayMetadata {
            id: "meta-1".to_string(),
            inference_id: request.request_id.clone(),
            tenant_id: request.cpid.clone(),
            manifest_hash: manifest.to_hex(),
            base_model_id: Some("base-model".to_string()),
            router_seed: Some(ctx_from_request.router_seed_hex().to_string()),
            sampling_params_json: serde_json::to_string(&sampling_params).unwrap(),
            backend: "Metal".to_string(),
            backend_version: Some("v1".to_string()),
            coreml_package_hash: None,
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: "v1".to_string(),
            rag_snapshot_hash: None,
            dataset_version_id: None,
            adapter_ids_json: None,
            base_only: None,
            prompt_text: "p".to_string(),
            prompt_truncated: 0,
            response_text: Some("r".to_string()),
            response_truncated: 0,
            rag_doc_ids_json: None,
            chat_context_hash: None,
            replay_status: "available".to_string(),
            latency_ms: Some(1),
            tokens_generated: Some(1),
            determinism_mode: Some("strict".to_string()),
            fallback_triggered: Some(false),
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            fallback_backend: None,
            replay_guarantee: Some("exact".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
            stop_policy_json: None,
            policy_mask_digest_b3: None,
            utf8_healing: None,
            created_at: "now".to_string(),
        };

        let ctx_from_replay =
            from_replay_metadata(&metadata).expect("replay context should derive");

        assert_eq!(
            ctx_from_request.request_seed(),
            ctx_from_replay.request_seed(),
            "Master seeds must round-trip"
        );
        assert_eq!(
            ctx_from_request.router_seed_hex(),
            ctx_from_replay.router_seed_hex(),
            "Router seeds must round-trip"
        );
        assert_eq!(
            ctx_from_request.sampler_seed(3),
            ctx_from_replay.sampler_seed(3),
            "Sampler seeds must be stable per step"
        );
        assert_eq!(
            ctx_from_replay.source(),
            &DeterminismSource::RequestSeedHex,
            "Replay path should respect explicit request_seed_hex"
        );
        assert_eq!(
            ctx_from_replay.routing_mode(),
            RoutingDeterminismMode::Deterministic,
            "Replays must force deterministic routing mode"
        );
        assert_eq!(
            ctx_from_replay.seed_mode(),
            SeedMode::BestEffort,
            "Seed mode should round-trip from replay metadata"
        );
    }

    #[test]
    fn replay_seed_expands_from_u64_seed() {
        let manifest = B3Hash::hash(b"manifest");
        let sampling_params = SamplingParams {
            temperature: 0.0,
            top_k: Some(4),
            top_p: Some(0.9),
            max_tokens: 16,
            seed: Some(42),
            error_code: None,
            seed_mode: Some(SeedMode::BestEffort),
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
            run_envelope: None,
            adapter_hashes_b3: None,
            dataset_hash_b3: None,
        };

        let metadata = InferenceReplayMetadata {
            id: "meta-2".to_string(),
            inference_id: "replay-seed-u64".to_string(),
            tenant_id: "tenant-1".to_string(),
            manifest_hash: manifest.to_hex(),
            base_model_id: None,
            router_seed: None,
            sampling_params_json: serde_json::to_string(&sampling_params).unwrap(),
            backend: "Metal".to_string(),
            backend_version: None,
            coreml_package_hash: None,
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: "v1".to_string(),
            rag_snapshot_hash: None,
            dataset_version_id: None,
            adapter_ids_json: None,
            base_only: None,
            prompt_text: "p".to_string(),
            prompt_truncated: 0,
            response_text: None,
            response_truncated: 0,
            rag_doc_ids_json: None,
            chat_context_hash: None,
            replay_status: "available".to_string(),
            latency_ms: None,
            tokens_generated: None,
            determinism_mode: Some("strict".to_string()),
            fallback_triggered: Some(false),
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            fallback_backend: None,
            replay_guarantee: Some("exact".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
            stop_policy_json: None,
            policy_mask_digest_b3: None,
            utf8_healing: None,
            created_at: "now".to_string(),
        };

        let ctx = from_replay_metadata(&metadata).expect("derive ctx");
        assert_eq!(
            ctx.source(),
            &DeterminismSource::SeedU64Expanded,
            "Should expand from legacy seed field"
        );
        assert_eq!(
            ctx.request_seed_low64(),
            42u64,
            "Lower 64 bits should mirror seed when expanded"
        );
    }

    #[test]
    fn seedless_replay_metadata_is_rejected() {
        let manifest = B3Hash::hash(b"manifest");
        let sampling_params = SamplingParams {
            temperature: 0.0,
            top_k: None,
            top_p: None,
            max_tokens: 8,
            seed: None,
            error_code: None,
            seed_mode: None,
            backend_profile: None,
            request_seed_hex: None,
            placement: None,
            run_envelope: None,
            adapter_hashes_b3: None,
            dataset_hash_b3: None,
        };

        let metadata = InferenceReplayMetadata {
            id: "meta-legacy".to_string(),
            inference_id: "legacy-inference".to_string(),
            tenant_id: "tenant-1".to_string(),
            manifest_hash: manifest.to_hex(),
            base_model_id: None,
            router_seed: None,
            sampling_params_json: serde_json::to_string(&sampling_params).unwrap(),
            backend: "Metal".to_string(),
            backend_version: None,
            coreml_package_hash: None,
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: "v1".to_string(),
            rag_snapshot_hash: None,
            dataset_version_id: None,
            adapter_ids_json: None,
            base_only: None,
            prompt_text: "p".to_string(),
            prompt_truncated: 0,
            response_text: None,
            response_truncated: 0,
            rag_doc_ids_json: None,
            chat_context_hash: None,
            replay_status: "available".to_string(),
            latency_ms: None,
            tokens_generated: None,
            determinism_mode: Some("besteffort".to_string()),
            fallback_triggered: Some(false),
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            fallback_backend: None,
            replay_guarantee: Some("approximate".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
            stop_policy_json: None,
            policy_mask_digest_b3: None,
            utf8_healing: None,
            created_at: "now".to_string(),
        };

        let err =
            from_replay_metadata(&metadata).expect_err("seedless replay metadata must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("missing request_seed"),
            "error should explain missing seeds; got {msg}"
        );
    }
}
