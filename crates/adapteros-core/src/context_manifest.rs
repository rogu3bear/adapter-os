//! Canonical context manifest bytes for determinism and replay.
//!
//! Versioned schema with stable, order- and encoding-safe serialization.

use crate::{B3Hash, FusionInterval, SeedMode};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

/// Adapter entry in canonical, ordered stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAdapterEntryV1 {
    pub adapter_id: String,
    pub adapter_hash: B3Hash,
    pub rank: u32,
    pub alpha_num: u64,
    pub alpha_den: u64,
    pub backend_id: String,
    pub kernel_version_id: String,
}

/// Versioned context manifest (schema_version = 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextManifestV1 {
    pub base_model_id: String,
    pub base_model_hash: B3Hash,
    pub adapter_dir_hash: B3Hash,
    pub adapter_stack: Vec<ContextAdapterEntryV1>,
    pub router_version: String,
    #[serde(default = "FusionInterval::default_mode")]
    pub fusion_interval: FusionInterval,
    pub seed_mode: SeedMode,
    pub seed_inputs_digest: B3Hash,
    pub policy_digest: B3Hash,
    pub sampler_params_digest: B3Hash,
    pub build_id: String,
    #[serde(default = "default_build_git_sha")]
    pub build_git_sha: String,
}

impl ContextManifestV1 {
    pub const SCHEMA_VERSION: u8 = 2;

    /// Serialize into canonical bytes (big-endian integers, UTF-8 strings).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(Self::SCHEMA_VERSION);

        encode_str(&mut bytes, &self.base_model_id);
        encode_hash(&mut bytes, &self.base_model_hash);
        encode_hash(&mut bytes, &self.adapter_dir_hash);

        encode_u32(
            &mut bytes,
            u32::try_from(self.adapter_stack.len()).expect("adapter_stack length fits in u32"),
        );
        for adapter in &self.adapter_stack {
            adapter.encode_into(&mut bytes);
        }

        encode_str(&mut bytes, &self.router_version);
        let (interval_tag, segment_len) = match self.fusion_interval {
            FusionInterval::PerRequest => ("per_request".to_string(), 0),
            FusionInterval::PerToken => ("per_token".to_string(), 0),
            FusionInterval::PerSegment { tokens_per_segment } => {
                ("per_segment".to_string(), tokens_per_segment)
            }
        };
        encode_str(&mut bytes, &interval_tag);
        encode_u32(&mut bytes, segment_len);
        encode_str(&mut bytes, self.seed_mode.as_str());
        encode_hash(&mut bytes, &self.seed_inputs_digest);
        encode_hash(&mut bytes, &self.policy_digest);
        encode_hash(&mut bytes, &self.sampler_params_digest);
        encode_str(&mut bytes, &self.build_id);
        encode_str(&mut bytes, &self.build_git_sha);

        bytes
    }

    /// Compute deterministic BLAKE3 digest of canonical bytes.
    pub fn digest(&self) -> B3Hash {
        B3Hash::hash(&self.to_bytes())
    }
}

impl ContextAdapterEntryV1 {
    fn encode_into(&self, out: &mut Vec<u8>) {
        encode_str(out, &self.adapter_id);
        encode_hash(out, &self.adapter_hash);
        encode_u32(out, self.rank);
        encode_u64(out, self.alpha_num);
        encode_u64(out, self.alpha_den);
        encode_str(out, &self.backend_id);
        encode_str(out, &self.kernel_version_id);
    }
}

fn encode_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn encode_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn encode_hash(buf: &mut Vec<u8>, hash: &B3Hash) {
    buf.extend_from_slice(hash.as_bytes());
}

fn encode_str(buf: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    let len = u32::try_from(bytes.len()).expect("string length fits in u32");
    encode_u32(buf, len);
    buf.extend_from_slice(bytes);
}

fn default_build_git_sha() -> String {
    crate::version::GIT_COMMIT_HASH.to_string()
}

pub type ContextAdapterEntry = ContextAdapterEntryV1;
pub type ContextManifest = ContextManifestV1;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_adapters() -> (ContextAdapterEntryV1, ContextAdapterEntryV1) {
        let a1 = ContextAdapterEntryV1 {
            adapter_id: "adapter-a".to_string(),
            adapter_hash: B3Hash::hash(b"adapter-a"),
            rank: 16,
            alpha_num: 3,
            alpha_den: 2,
            backend_id: "coreml".to_string(),
            kernel_version_id: "metal-k1".to_string(),
        };
        let a2 = ContextAdapterEntryV1 {
            adapter_id: "adapter-b".to_string(),
            adapter_hash: B3Hash::hash(b"adapter-b"),
            rank: 8,
            alpha_num: 1,
            alpha_den: 1,
            backend_id: "mlx".to_string(),
            kernel_version_id: "mlx-k2".to_string(),
        };
        (a1, a2)
    }

    fn sample_manifest(stack: Vec<ContextAdapterEntryV1>) -> ContextManifestV1 {
        ContextManifestV1 {
            base_model_id: "qwen2.5-7b".to_string(),
            base_model_hash: B3Hash::hash(b"base-model"),
            adapter_dir_hash: B3Hash::hash(b"adapter-dir"),
            adapter_stack: stack,
            router_version: "router-1.0.0".to_string(),
            fusion_interval: FusionInterval::PerRequest,
            seed_mode: SeedMode::Strict,
            seed_inputs_digest: B3Hash::hash(b"seed-inputs"),
            policy_digest: B3Hash::hash(b"policy-digest"),
            sampler_params_digest: B3Hash::hash(b"sampler-params"),
            build_id: "build-123".to_string(),
            build_git_sha: "git-sha-abc123".to_string(),
        }
    }

    #[test]
    fn same_struct_values_produce_same_bytes() {
        let (a1, a2) = sample_adapters();
        let m1 = sample_manifest(vec![a1.clone(), a2.clone()]);
        let m2 = sample_manifest(vec![a1, a2]);

        assert_eq!(m1.to_bytes(), m2.to_bytes());
    }

    #[test]
    fn same_bytes_produce_same_digest() {
        let (a1, a2) = sample_adapters();
        let manifest = sample_manifest(vec![a1, a2]);

        let bytes = manifest.to_bytes();
        assert_eq!(manifest.digest(), B3Hash::hash(&bytes));
    }

    #[test]
    fn adapter_order_changes_digest() {
        let (a1, a2) = sample_adapters();
        let ordered = sample_manifest(vec![a1.clone(), a2.clone()]);
        let reversed = sample_manifest(vec![a2, a1]);

        assert_ne!(ordered.digest(), reversed.digest());
    }

    #[test]
    fn build_id_changes_digest() {
        let (a1, a2) = sample_adapters();
        let mut manifest = sample_manifest(vec![a1, a2]);
        let d1 = manifest.digest();

        manifest.build_id = "build-456".to_string();
        let d2 = manifest.digest();

        assert_ne!(d1, d2);
    }

    #[test]
    fn git_sha_changes_digest() {
        let (a1, a2) = sample_adapters();
        let mut manifest = sample_manifest(vec![a1, a2]);
        let baseline = manifest.digest();

        manifest.build_git_sha = "git-sha-def456".to_string();
        let changed = manifest.digest();

        assert_ne!(baseline, changed);
    }

    #[test]
    fn fusion_interval_changes_digest() {
        let (a1, a2) = sample_adapters();
        let mut manifest = sample_manifest(vec![a1, a2]);
        let d1 = manifest.digest();

        manifest.fusion_interval = FusionInterval::PerToken;
        let d2 = manifest.digest();

        assert_ne!(d1, d2);
    }
}
