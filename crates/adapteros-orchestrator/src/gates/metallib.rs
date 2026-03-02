//! Metallib gate: verifies embedded metallib hash matches manifest

use crate::{DependencyChecker, Gate, OrchestratorConfig};
use adapteros_core::B3Hash;
use adapteros_core::{AosError, Result};
use adapteros_model_hub::manifest::ManifestV3;
use std::fs;
use std::path::Path;
use tracing::{debug, warn};

#[derive(Debug, Clone, Default)]
pub struct MetallibGate;

#[async_trait::async_trait]
impl Gate for MetallibGate {
    fn name(&self) -> String {
        "Metal Kernel Hash".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        // Check dependencies first
        let checker = DependencyChecker::new();
        let deps = checker.check_gate("metallib")?;

        if !deps.all_available {
            debug!(messages = ?deps.messages, "Some metallib dependencies missing");
        }

        // Load manifest for CPID
        let manifests_dir = if let Some(resolved) = deps.get_resolved_path("manifests_dir") {
            resolved
        } else {
            config.manifests_path.clone()
        };

        let manifest_path = Path::new(&manifests_dir).join(format!("{}.yaml", config.cpid));

        if !manifest_path.exists() {
            // Try JSON
            let manifest_path = Path::new(&manifests_dir).join(format!("{}.json", config.cpid));

            if !manifest_path.exists() {
                return Err(AosError::NotFound(format!(
                    "Manifest not found for CPID: {} (checked {} and {})",
                    config.cpid, manifests_dir, &config.manifests_path
                )));
            }
        }

        let manifest_content = fs::read_to_string(&manifest_path)
            .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;

        let _manifest: ManifestV3 =
            if manifest_path.extension().and_then(|s| s.to_str()) == Some("json") {
                serde_json::from_str(&manifest_content)?
            } else {
                serde_yaml::from_str(&manifest_content)
                    .map_err(|e| AosError::Parse(format!("Failed to parse YAML manifest: {}", e)))?
            };

        // Get kernel hash from plan (stored in database)
        let db = adapteros_db::Db::connect(&config.db_path).await?;

        let plan = sqlx::query_as::<_, adapteros_db::models::Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
             FROM plans
             WHERE plan_id_b3 = ?
             LIMIT 1"
        )
        .bind(&config.cpid)
        .fetch_optional(db.pool_result()?)
        .await?
        .ok_or_else(|| AosError::NotFound(format!("No plan found for CPID: {}", config.cpid)))?;

        let expected_hash = plan
            .metallib_hash_b3
            .ok_or_else(|| AosError::Internal("No metallib_hash_b3 in plan".to_string()))?;

        // Check if metallib exists with fallback paths
        let metallib_path =
            Path::new("crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib");

        let resolved_path = if metallib_path.exists() {
            Some(metallib_path.to_path_buf())
        } else {
            // Try alternate paths
            let alt_paths = [
                "crates/mplora-kernel-mtl/shaders/aos_kernels.metallib",
                "target/shaders/aos_kernels.metallib",
            ];

            alt_paths.iter().find_map(|p| {
                let path = Path::new(p);
                if path.exists() {
                    Some(path.to_path_buf())
                } else {
                    None
                }
            })
        };

        let metallib_path = match resolved_path {
            Some(path) => {
                if path != metallib_path {
                    warn!("Using alternate metallib path: {}", path.display());
                }
                path
            }
            None => {
                return Err(AosError::NotFound(format!(
                    "Metal kernel library not found: {} (and alternate paths not found)",
                    metallib_path.display()
                )));
            }
        };

        let metallib_bytes = fs::read(&metallib_path)
            .map_err(|e| AosError::Io(format!("Failed to read metallib: {}", e)))?;

        let actual_hash = B3Hash::hash(&metallib_bytes);
        let expected = B3Hash::from_hex(&expected_hash).map_err(|e| {
            AosError::InvalidHash(format!("Invalid kernel hash in manifest: {}", e))
        })?;

        if actual_hash != expected {
            return Err(AosError::Verification(format!(
                "Kernel hash mismatch: expected {}, got {}",
                expected_hash,
                actual_hash.to_hex()
            )));
        }

        tracing::info!(kernel_hash = %expected_hash, "Kernel hash verified");

        Ok(())
    }
}
