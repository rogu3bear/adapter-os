use crate::errors::InferenceError;
use crate::types::{InferenceRequestInternal, SamplingParams};
use adapteros_core::{derive_request_seed, B3Hash, SeedMode};
use adapteros_db::InferenceReplayMetadata;
use blake3::Hasher;
use hkdf::Hkdf;
use sha2::Sha256;

/// Canonical determinism context derived from a request or replay metadata.
///
/// Ensures that request-scoped seeds and router seeds are computed identically
/// for live requests and replays.
#[derive(Debug, Clone)]
pub struct DeterminismContext {
    request_seed: [u8; 32],
    request_seed_low64: u64,
    router_seed_hex: String,
}

impl DeterminismContext {
    /// Build determinism context from an in-flight request.
    pub fn from_request(
        request: &InferenceRequestInternal,
        manifest_hash: Option<&B3Hash>,
        global_seed: &B3Hash,
        seed_mode: SeedMode,
        worker_id: u32,
    ) -> Result<Self, InferenceError> {
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

        let router_seed_hex = derive_router_seed(&request_seed, manifest_hash);
        let request_seed_low64 = u64::from_le_bytes(request_seed[..8].try_into().unwrap());

        tracing::debug!(
            request_id = %request.request_id,
            router_seed = %router_seed_hex,
            request_seed_hex = %hex::encode(request_seed),
            "DeterminismContext derived"
        );

        Ok(Self {
            request_seed,
            request_seed_low64,
            router_seed_hex,
        })
    }

    /// Build determinism context from persisted replay metadata.
    pub fn from_replay_metadata(
        metadata: &InferenceReplayMetadata,
    ) -> Result<Self, InferenceError> {
        let sampling_params: SamplingParams = serde_json::from_str(&metadata.sampling_params_json)
            .map_err(|e| {
                InferenceError::ValidationError(format!(
                    "Failed to parse sampling params from replay metadata: {}",
                    e
                ))
            })?;

        let request_seed = if let Some(hex_seed) = sampling_params.request_seed_hex {
            let bytes = hex::decode(hex_seed).map_err(|e| {
                InferenceError::ValidationError(format!(
                    "Invalid request_seed_hex in replay metadata: {}",
                    e
                ))
            })?;
            bytes.try_into().map_err(|_| {
                InferenceError::ValidationError("request_seed_hex must be 32 bytes".to_string())
            })?
        } else if let Some(seed64) = sampling_params.seed {
            expand_u64_seed(seed64)
        } else {
            return Err(InferenceError::ValidationError(
                "Replay metadata missing request_seed_hex and seed".to_string(),
            ));
        };

        let manifest_hash = B3Hash::from_hex(&metadata.manifest_hash).ok();
        let router_seed_hex = derive_router_seed(&request_seed, manifest_hash.as_ref());
        let request_seed_low64 = u64::from_le_bytes(request_seed[..8].try_into().unwrap());

        tracing::debug!(
            inference_id = %metadata.inference_id,
            router_seed = %router_seed_hex,
            request_seed_hex = %hex::encode(request_seed),
            "DeterminismContext reconstructed from replay metadata"
        );

        Ok(Self {
            request_seed,
            request_seed_low64,
            router_seed_hex,
        })
    }

    /// Get master request seed bytes.
    pub fn request_seed(&self) -> [u8; 32] {
        self.request_seed
    }

    /// Get the lower 64 bits of the master seed for API compatibility.
    pub fn request_seed_low64(&self) -> u64 {
        self.request_seed_low64
    }

    /// Get router seed hex string.
    pub fn router_seed_hex(&self) -> &str {
        &self.router_seed_hex
    }

    /// Derive per-step sampler seed following the canonical rule.
    pub fn sampler_seed(&self, step: u64) -> [u8; 32] {
        derive_sampler_seed(&self.request_seed, step)
    }
}

fn derive_router_seed(request_seed: &[u8; 32], manifest_hash: Option<&B3Hash>) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"router");
    hasher.update(request_seed);
    if let Some(hash) = manifest_hash {
        hasher.update(hash.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

pub fn derive_sampler_seed(request_seed: &[u8; 32], step: u64) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(b"sample");
    hasher.update(request_seed);
    hasher.update(&step.to_le_bytes());
    hasher.finalize().as_bytes().to_owned().try_into().unwrap()
}

fn expand_u64_seed(seed: u64) -> [u8; 32] {
    let mut seed_bytes = [0u8; 32];
    seed_bytes[..8].copy_from_slice(&seed.to_le_bytes());
    let hk = Hkdf::<Sha256>::new(None, &seed_bytes[..8]);
    hk.expand(b"replay-seed-expand", &mut seed_bytes)
        .expect("HKDF expand failed");
    seed_bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::SeedMode;
    use adapteros_core::B3Hash;
    use adapteros_db::InferenceReplayMetadata;

    #[test]
    fn replay_round_trip_preserves_seeds() {
        let manifest = B3Hash::hash(b"manifest");
        let global = B3Hash::hash(b"global");

        let mut request = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
        request.request_id = "req-123".to_string();

        let ctx_from_request = DeterminismContext::from_request(
            &request,
            Some(&manifest),
            &global,
            SeedMode::BestEffort,
            7,
        )
        .expect("request context should derive");

        let sampling_params = SamplingParams {
            temperature: 0.0,
            top_k: Some(4),
            top_p: Some(0.9),
            max_tokens: 16,
            seed: Some(ctx_from_request.request_seed_low64()),
            seed_mode: Some(SeedMode::BestEffort),
            backend_profile: None,
            request_seed_hex: Some(hex::encode(ctx_from_request.request_seed())),
            placement: None,
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
            sampling_algorithm_version: "v1".to_string(),
            rag_snapshot_hash: None,
            adapter_ids_json: None,
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
            replay_guarantee: Some("exact".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
            created_at: "now".to_string(),
        };

        let ctx_from_replay =
            DeterminismContext::from_replay_metadata(&metadata).expect("replay context should derive");

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
    }
}
