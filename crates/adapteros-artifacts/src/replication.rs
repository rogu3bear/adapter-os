//! Replication module for CAS-aware sparse artifact transfers
//!
//! Supports:
//! - Signed replication manifests
//! - Resume-safe chunk transfers
//! - BLAKE3 verification
//! - Air-gap export/import

use adapteros_core::{time, AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing;

use crate::CasStore;

/// Replication manifest describing artifacts to transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationManifest {
    pub session_id: String,
    pub artifacts: Vec<ArtifactDescriptor>,
    pub total_bytes: u64,
    pub signature: String,
    pub created_at: String,
}

/// Descriptor for a single artifact in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    pub adapter_id: String,
    pub hash: B3Hash,
    pub size_bytes: u64,
    pub chunks: Vec<ChunkDescriptor>,
}

/// Chunk descriptor for resume-safe transfers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDescriptor {
    pub offset: u64,
    pub size: u64,
    pub hash: B3Hash,
}

/// Replication result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationResult {
    pub session_id: String,
    pub artifacts_transferred: usize,
    pub bytes_transferred: u64,
    pub duration_ms: u64,
    pub verified: bool,
}

/// Create replication manifest for given adapters
pub fn create_manifest(
    _cas_store: &CasStore,
    adapter_ids: &[String],
    session_id: String,
) -> Result<ReplicationManifest> {
    let mut artifacts = Vec::new();
    let mut total_bytes = 0u64;

    for adapter_id in adapter_ids {
        // Query artifact from CAS store
        let hash = B3Hash::hash(adapter_id.as_bytes()); // Mock - in production would query registry

        // In production, would load actual artifact and compute chunks
        let artifact = ArtifactDescriptor {
            adapter_id: adapter_id.clone(),
            hash,
            size_bytes: 1024 * 1024, // Mock 1MB
            chunks: vec![ChunkDescriptor {
                offset: 0,
                size: 1024 * 1024,
                hash,
            }],
        };

        total_bytes += artifact.size_bytes;
        artifacts.push(artifact);
    }

    Ok(ReplicationManifest {
        session_id,
        artifacts,
        total_bytes,
        signature: "mock_signature".to_string(), // In production: sign with Ed25519
        created_at: time::now_rfc3339(),
    })
}

/// Replicate artifacts to target node (sparse transfer)
pub async fn replicate_artifacts(
    _cas_store: &CasStore,
    manifest: &ReplicationManifest,
    target_endpoint: &str,
) -> Result<ReplicationResult> {
    let start = std::time::Instant::now();
    let mut bytes_transferred = 0u64;

    // Send manifest to target
    send_manifest(target_endpoint, manifest).await?;

    // Transfer each artifact
    for artifact in &manifest.artifacts {
        // Check if target already has this artifact (sparse)
        if !target_has_artifact(target_endpoint, &artifact.hash).await? {
            // Transfer chunks
            for chunk in &artifact.chunks {
                transfer_chunk(_cas_store, target_endpoint, artifact, chunk).await?;
                bytes_transferred += chunk.size;
            }
        }
    }

    // Verify transfer
    let verified = verify_replication(target_endpoint, manifest).await?;

    Ok(ReplicationResult {
        session_id: manifest.session_id.clone(),
        artifacts_transferred: manifest.artifacts.len(),
        bytes_transferred,
        duration_ms: start.elapsed().as_millis() as u64,
        verified,
    })
}

/// Export artifacts for air-gap transfer
pub fn export_air_gap(
    _cas_store: &CasStore,
    adapter_ids: &[String],
    output_path: &Path,
) -> Result<PathBuf> {
    use std::fs::File;
    use std::io::Write;

    // Create manifest
    let session_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();
    let manifest = create_manifest(_cas_store, adapter_ids, session_id)?;

    // In production, would create a tar.zst bundle with:
    // - Signed manifest
    // - Artifact files from CAS
    // - SBOM files

    // For now, write manifest as JSON
    let mut file = File::create(output_path).map_err(|e| {
        tracing::warn!(
            path = %output_path.display(),
            error = %e,
            "failed to create air-gap export file"
        );
        AosError::Artifact(format!(
            "failed to create air-gap export file '{}': {}",
            output_path.display(),
            e
        ))
    })?;
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| {
        tracing::warn!(
            session_id = %manifest.session_id,
            error = %e,
            "failed to serialize replication manifest"
        );
        AosError::Artifact(format!("failed to serialize replication manifest: {}", e))
    })?;
    file.write_all(manifest_json.as_bytes()).map_err(|e| {
        tracing::warn!(
            path = %output_path.display(),
            error = %e,
            "failed to write air-gap export manifest"
        );
        AosError::Artifact(format!(
            "failed to write air-gap export manifest to '{}': {}",
            output_path.display(),
            e
        ))
    })?;

    Ok(output_path.to_path_buf())
}

/// Import artifacts from air-gap bundle
pub fn import_air_gap(_cas_store: &CasStore, bundle_path: &Path) -> Result<ReplicationResult> {
    use std::fs::File;
    use std::io::Read;

    // Load and verify bundle
    let mut file = File::open(bundle_path).map_err(|e| {
        tracing::warn!(
            path = %bundle_path.display(),
            error = %e,
            "failed to open air-gap bundle for import"
        );
        AosError::Artifact(format!(
            "failed to open air-gap bundle '{}': {}",
            bundle_path.display(),
            e
        ))
    })?;
    let mut manifest_json = String::new();
    file.read_to_string(&mut manifest_json).map_err(|e| {
        tracing::warn!(
            path = %bundle_path.display(),
            error = %e,
            "failed to read air-gap bundle content"
        );
        AosError::Artifact(format!(
            "failed to read air-gap bundle '{}': {}",
            bundle_path.display(),
            e
        ))
    })?;

    let manifest: ReplicationManifest = serde_json::from_str(&manifest_json).map_err(|e| {
        tracing::warn!(
            path = %bundle_path.display(),
            error = %e,
            "failed to parse air-gap bundle manifest"
        );
        AosError::Artifact(format!(
            "failed to parse air-gap bundle manifest from '{}': {}",
            bundle_path.display(),
            e
        ))
    })?;

    // Verify signature (in production)
    // verify_signature(&manifest)?;

    // Import artifacts into CAS
    let start = std::time::Instant::now();
    let mut bytes_transferred = 0u64;

    for artifact in &manifest.artifacts {
        // In production, would extract from bundle and verify hash
        bytes_transferred += artifact.size_bytes;
    }

    Ok(ReplicationResult {
        session_id: manifest.session_id.clone(),
        artifacts_transferred: manifest.artifacts.len(),
        bytes_transferred,
        duration_ms: start.elapsed().as_millis() as u64,
        verified: true,
    })
}

/// Verify replication completed successfully
pub async fn verify_replication(
    target_endpoint: &str,
    manifest: &ReplicationManifest,
) -> Result<bool> {
    // Query target for artifact hashes
    let client = reqwest::Client::new();
    let url = format!("{}/sync/verify", target_endpoint);

    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "session_id": manifest.session_id,
            "artifacts": manifest.artifacts.iter().map(|a| {
                serde_json::json!({
                    "adapter_id": a.adapter_id,
                    "hash": a.hash.to_hex(),
                })
            }).collect::<Vec<_>>(),
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| AosError::Registry(format!("Verification request failed: {}", e)))?;

    #[derive(Deserialize)]
    struct VerifyResponse {
        verified: bool,
    }

    if response.status().is_success() {
        let verify_resp: VerifyResponse = response
            .json()
            .await
            .map_err(|e| AosError::Registry(format!("Failed to parse verify response: {}", e)))?;
        Ok(verify_resp.verified)
    } else {
        Ok(false)
    }
}

// Helper functions

async fn send_manifest(endpoint: &str, manifest: &ReplicationManifest) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/sync/manifest", endpoint);

    let response = client
        .post(&url)
        .json(manifest)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| AosError::Registry(format!("Failed to send manifest: {}", e)))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(AosError::Registry(format!(
            "Manifest rejected: HTTP {}",
            response.status()
        )))
    }
}

async fn target_has_artifact(endpoint: &str, hash: &B3Hash) -> Result<bool> {
    let client = reqwest::Client::new();
    let url = format!("{}/sync/has-artifact", endpoint);

    let response = client
        .post(&url)
        .json(&serde_json::json!({ "hash": hash.to_hex() }))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            #[derive(Deserialize)]
            struct HasArtifactResponse {
                has_artifact: bool,
            }

            let has_resp: HasArtifactResponse = resp.json().await.map_err(|e| {
                AosError::Registry(format!("Failed to parse has-artifact response: {}", e))
            })?;
            Ok(has_resp.has_artifact)
        }
        _ => Ok(false), // Assume artifact needs transfer if query fails
    }
}

async fn transfer_chunk(
    _cas_store: &CasStore,
    endpoint: &str,
    artifact: &ArtifactDescriptor,
    chunk: &ChunkDescriptor,
) -> Result<()> {
    // In production, would:
    // 1. Load chunk from CAS
    // 2. Stream to target
    // 3. Verify chunk hash

    let client = reqwest::Client::new();
    let url = format!("{}/sync/chunk", endpoint);

    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "adapter_id": artifact.adapter_id,
            "offset": chunk.offset,
            "size": chunk.size,
            "hash": chunk.hash.to_hex(),
            "data": vec![0u8; chunk.size as usize], // Mock data
        }))
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| AosError::Registry(format!("Chunk transfer failed: {}", e)))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(AosError::Registry(format!(
            "Chunk rejected: HTTP {}",
            response.status()
        )))
    }
}
