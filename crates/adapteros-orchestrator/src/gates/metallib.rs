//! Metallib gate: verifies embedded metallib hash matches manifest

use crate::{Gate, OrchestratorConfig};
use anyhow::{Context, Result};
use adapteros_core::B3Hash;
use adapteros_manifest::ManifestV3;
use std::fs;
use std::path::Path;

#[derive(Debug, Default)]
pub struct MetallibGate;

#[async_trait::async_trait]
impl Gate for MetallibGate {
    fn name(&self) -> String {
        "Metal Kernel Hash".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        // Load manifest for CPID
        let manifest_path = Path::new(&config.manifests_path).join(format!("{}.yaml", config.cpid));

        if !manifest_path.exists() {
            // Try JSON
            let manifest_path =
                Path::new(&config.manifests_path).join(format!("{}.json", config.cpid));

            if !manifest_path.exists() {
                anyhow::bail!("Manifest not found for CPID: {}", config.cpid);
            }
        }

        let manifest_content =
            fs::read_to_string(&manifest_path).context("Failed to read manifest")?;

        let _manifest: ManifestV3 =
            if manifest_path.extension().and_then(|s| s.to_str()) == Some("json") {
                serde_json::from_str(&manifest_content)?
            } else {
                serde_yaml::from_str(&manifest_content)?
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
        .fetch_optional(db.pool())
        .await?
        .ok_or_else(|| anyhow::anyhow!("No plan found for CPID: {}", config.cpid))?;

        let expected_hash = plan
            .metallib_hash_b3
            .ok_or_else(|| anyhow::anyhow!("No metallib_hash_b3 in plan"))?;

        // Check if metallib exists and hash matches
        let metallib_path = Path::new("crates/mplora-kernel-mtl/shaders/aos_kernels.metallib");

        if !metallib_path.exists() {
            anyhow::bail!(
                "Metal kernel library not found: {}",
                metallib_path.display()
            );
        }

        let metallib_bytes = fs::read(metallib_path).context("Failed to read metallib")?;

        let actual_hash = B3Hash::hash(&metallib_bytes);
        let expected =
            B3Hash::from_hex(&expected_hash).context("Invalid kernel hash in manifest")?;

        if actual_hash != expected {
            anyhow::bail!(
                "Kernel hash mismatch: expected {}, got {}",
                expected_hash,
                actual_hash.to_hex()
            );
        }

        println!("    Kernel hash: {} ✓", expected_hash);

        Ok(())
    }
}
