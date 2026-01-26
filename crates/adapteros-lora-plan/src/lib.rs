//! Plan builder and model loading

use adapteros_core::{B3Hash, Result};
use adapteros_model_hub::manifest::ManifestV3;
use serde::{Deserialize, Serialize};

pub mod chat;
pub mod config;
pub mod layout;
pub mod loader;
pub mod quant;

pub use chat::*;
pub use config::*;
pub use layout::TensorLayout;
pub use loader::ModelLoader;
pub use quant::*;

/// Plan metadata with hashes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanMeta {
    pub manifest_hash: B3Hash,
    pub kernel_hashes: Vec<B3Hash>,
    pub layout_hash: B3Hash,
    pub plan_id: B3Hash,
    pub toolchain_version: String,
    pub rustc_version: String,
}

impl PlanMeta {
    /// Compute Plan ID from manifest, kernels, and layout
    pub fn compute_plan_id(
        manifest_hash: &B3Hash,
        kernel_hashes: &[B3Hash],
        layout_hash: &B3Hash,
    ) -> B3Hash {
        let mut all_bytes = Vec::new();
        all_bytes.extend_from_slice(manifest_hash.as_bytes());
        for kh in kernel_hashes {
            all_bytes.extend_from_slice(kh.as_bytes());
        }
        all_bytes.extend_from_slice(layout_hash.as_bytes());

        B3Hash::hash(&all_bytes)
    }
}

/// Build a plan from manifest and kernel library
pub fn build_plan(manifest: &ManifestV3, metallib: &[u8]) -> Result<PlanMeta> {
    // Hash manifest
    let manifest_hash = manifest.compute_hash()?;

    // Hash kernels
    let kernel_hashes = vec![B3Hash::hash(metallib)];

    // Compute layout hash
    let layout = TensorLayout::from_manifest(manifest)?;
    let layout_bytes = serde_json::to_vec(&layout)?;
    let layout_hash = B3Hash::hash(&layout_bytes);

    // Compute Plan ID
    let plan_id = PlanMeta::compute_plan_id(&manifest_hash, &kernel_hashes, &layout_hash);

    // Get toolchain info
    let toolchain_version = env!("CARGO_PKG_VERSION").to_string();
    let rustc_version = option_env!("RUSTC_VERSION")
        .unwrap_or("unknown")
        .to_string();

    Ok(PlanMeta {
        manifest_hash,
        kernel_hashes,
        layout_hash,
        plan_id,
        toolchain_version,
        rustc_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_id_deterministic() {
        let manifest_hash = B3Hash::hash(b"manifest");
        let kernel_hashes = vec![B3Hash::hash(b"kernel")];
        let layout_hash = B3Hash::hash(b"layout");

        let id1 = PlanMeta::compute_plan_id(&manifest_hash, &kernel_hashes, &layout_hash);
        let id2 = PlanMeta::compute_plan_id(&manifest_hash, &kernel_hashes, &layout_hash);

        assert_eq!(id1, id2);
    }
}
