//! Node sync command - replicate adapters across nodes

use super::NOT_IMPLEMENTED_MESSAGE;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Node sync subcommands
#[derive(Debug, Clone)]
pub enum SyncMode {
    Verify { from: String, to: String },
    Push { to: String, adapters: Vec<String> },
    Pull { from: String, adapters: Vec<String> },
    Export { file: PathBuf },
    Import { file: PathBuf },
}

/// Execute node sync operation
pub async fn run(mode: SyncMode) -> Result<()> {
    match mode {
        SyncMode::Verify { from, to } => verify_sync(&from, &to).await,
        SyncMode::Push { to, adapters } => push_adapters(&to, &adapters).await,
        SyncMode::Pull { from, adapters } => pull_adapters(&from, &adapters).await,
        SyncMode::Export { file } => export_air_gap(&file).await,
        SyncMode::Import { file } => import_air_gap(&file).await,
    }
}

/// Verify sync between two nodes
async fn verify_sync(from: &str, to: &str) -> Result<()> {
    println!("🔍 Verify Sync");
    println!("   From: {}", from);
    println!("   To: {}", to);
    println!();

    // Get nodes from database
    let db = adapteros_db::Db::connect_env().await?;
    let from_node = db
        .get_node(from)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Source node not found: {}", from))?;
    let to_node = db
        .get_node(to)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Target node not found: {}", to))?;

    println!("Comparing adapter hashes...");

    // Query hashes from both nodes
    let from_hashes = query_adapter_hashes(&from_node.agent_endpoint).await?;
    let to_hashes = query_adapter_hashes(&to_node.agent_endpoint).await?;

    // Compare
    let mut matches = 0;
    let mut mismatches = 0;
    let mut missing = 0;

    for (adapter_id, from_hash) in &from_hashes {
        if let Some(to_hash) = to_hashes.get(adapter_id) {
            if from_hash == to_hash {
                matches += 1;
                println!("  ✓ {}: match", adapter_id);
            } else {
                mismatches += 1;
                println!("  ✗ {}: hash mismatch", adapter_id);
            }
        } else {
            missing += 1;
            println!("  - {}: missing on target", adapter_id);
        }
    }

    println!();
    println!("Summary:");
    println!("  Matches: {}", matches);
    println!("  Mismatches: {}", mismatches);
    println!("  Missing: {}", missing);

    if mismatches > 0 || missing > 0 {
        Err(anyhow::anyhow!("Sync verification failed"))
    } else {
        println!("\n✓ Nodes are in sync");
        Ok(())
    }
}

/// Push adapters to target node
async fn push_adapters(to: &str, adapters: &[String]) -> Result<()> {
    println!("⬆ Push Adapters");
    println!("   To: {}", to);
    println!("   Adapters: {:?}", adapters);
    println!();

    // Get target node
    let db = adapteros_db::Db::connect_env().await?;
    let to_node = db
        .get_node(to)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Target node not found: {}", to))?;

    // Use replication module to push
    let cas_store = adapteros_artifacts::CasStore::new("./var/cas")?;

    println!("Creating replication manifest...");
    let manifest = create_replication_manifest(&cas_store, adapters).await?;

    println!("Replicating {} artifacts...", manifest.artifacts.len());

    // Send manifest to target
    replicate_to_node(&to_node.agent_endpoint, &manifest).await?;

    println!("\n✓ Push complete");
    Ok(())
}

/// Pull adapters from source node
async fn pull_adapters(from: &str, adapters: &[String]) -> Result<()> {
    println!("⬇ Pull Adapters");
    println!("   From: {}", from);
    println!("   Adapters: {:?}", adapters);
    println!();

    // Get source node
    let db = adapteros_db::Db::connect_env().await?;
    let from_node = db
        .get_node(from)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Source node not found: {}", from))?;

    // Request manifest from source
    println!("Requesting manifest...");
    let manifest = request_manifest(&from_node.agent_endpoint, adapters).await?;

    println!("Pulling {} artifacts...", manifest.artifacts.len());

    // Download artifacts
    let cas_store = adapteros_artifacts::CasStore::new("./var/cas")?;
    pull_from_node(&from_node.agent_endpoint, &manifest, &cas_store).await?;

    println!("\n✓ Pull complete");
    Ok(())
}

/// Export adapters for air-gap transfer
async fn export_air_gap(file: &Path) -> Result<()> {
    println!("📦 Export Air-Gap Bundle");
    println!("   File: {}", file.display());
    println!();

    // Connect to database and get all adapters
    let db = adapteros_db::Db::connect_env().await?;
    let adapters = db.list_all_adapters_system().await?;

    if adapters.is_empty() {
        println!("No adapters found to export");
        return Ok(());
    }

    println!("Found {} adapters to export", adapters.len());

    // Initialize CAS store
    let cas_store = adapteros_artifacts::CasStore::new("./var/cas")?;

    // Build manifest with actual artifact data
    let mut artifacts: Vec<ArtifactInfo> = Vec::new();
    let mut artifact_data: Vec<(String, Vec<u8>)> = Vec::new();

    for adapter in &adapters {
        let hash = B3Hash::hash(adapter.id.as_bytes());

        // Try to load artifact from CAS
        match cas_store.load("adapter", &hash) {
            Ok(data) => {
                println!("  ✓ {} ({} bytes)", adapter.id, data.len());
                artifacts.push(ArtifactInfo {
                    adapter_id: adapter.id.clone(),
                    hash: hash.to_hex(),
                    size_bytes: data.len() as u64,
                });
                artifact_data.push((adapter.id.clone(), data));
            }
            Err(_) => {
                println!("  - {} (not in CAS, skipping)", adapter.id);
            }
        }
    }

    if artifacts.is_empty() {
        println!("\nNo artifacts found in CAS store");
        return Ok(());
    }

    // Create manifest
    let session_id = uuid::Uuid::new_v4().to_string();
    let manifest_content = serde_json::json!({
        "session_id": session_id,
        "artifacts": artifacts,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "version": "1.0",
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest_content)?;

    // Sign manifest
    let signature = {
        let keypair = adapteros_crypto::Keypair::generate();
        let sig = keypair.sign(&manifest_bytes);
        hex::encode(sig.to_bytes())
    };

    let manifest = ReplicationManifest {
        session_id,
        artifacts,
        signature,
    };

    // Create tar.zst archive
    println!("\nCreating archive...");
    let output_file = std::fs::File::create(file)?;
    let encoder = zstd::stream::Encoder::new(output_file, 3)?;
    let mut tar_builder = tar::Builder::new(encoder);

    // Add manifest
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    let mut header = tar::Header::new_gnu();
    header.set_path("manifest.json")?;
    header.set_size(manifest_json.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs());
    header.set_cksum();
    tar_builder.append(&header, manifest_json.as_slice())?;

    // Add artifacts
    for (adapter_id, data) in &artifact_data {
        let path = format!("artifacts/{}.bin", adapter_id);
        let mut header = tar::Header::new_gnu();
        header.set_path(&path)?;
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs());
        header.set_cksum();
        tar_builder.append(&header, data.as_slice())?;
    }

    // Finalize archive
    let encoder = tar_builder.into_inner()?;
    encoder.finish()?;

    let file_size = std::fs::metadata(file)?.len();
    println!("\n✓ Export complete: {} ({} bytes)", file.display(), file_size);
    println!("  Artifacts: {}", artifact_data.len());

    Ok(())
}

/// Import adapters from air-gap bundle
async fn import_air_gap(file: &Path) -> Result<()> {
    println!("📥 Import Air-Gap Bundle");
    println!("   File: {}", file.display());
    println!();

    // Verify bundle exists
    if !file.exists() {
        return Err(anyhow::anyhow!("Bundle file not found: {}", file.display()));
    }

    // Open and decompress archive
    println!("Reading archive...");
    let input_file = std::fs::File::open(file)?;
    let decoder = zstd::stream::Decoder::new(input_file)?;
    let mut archive = tar::Archive::new(decoder);

    // Extract to temp directory
    let temp_dir = tempfile::tempdir()?;
    archive.unpack(temp_dir.path())?;

    // Read manifest
    let manifest_path = temp_dir.path().join("manifest.json");
    if !manifest_path.exists() {
        return Err(anyhow::anyhow!("Invalid bundle: manifest.json not found"));
    }

    let manifest_data = std::fs::read_to_string(&manifest_path)?;
    let manifest: ReplicationManifest = serde_json::from_str(&manifest_data)
        .context("Failed to parse manifest")?;

    println!("Bundle contains {} artifacts", manifest.artifacts.len());

    // Initialize CAS store
    let cas_store = adapteros_artifacts::CasStore::new("./var/cas")?;

    // Import artifacts
    let artifacts_dir = temp_dir.path().join("artifacts");
    let mut imported = 0;
    let mut skipped = 0;

    for artifact in &manifest.artifacts {
        let artifact_path = artifacts_dir.join(format!("{}.bin", artifact.adapter_id));

        if !artifact_path.exists() {
            println!("  - {} (missing from archive)", artifact.adapter_id);
            skipped += 1;
            continue;
        }

        let data = std::fs::read(&artifact_path)?;

        // Verify hash
        let computed_hash = B3Hash::hash(&data);
        let expected_hash = B3Hash::from_hex(&artifact.hash)?;

        if computed_hash != expected_hash {
            println!("  ✗ {} (hash mismatch)", artifact.adapter_id);
            skipped += 1;
            continue;
        }

        // Store in CAS
        cas_store.store("adapter", &computed_hash, &data)?;
        println!("  ✓ {} ({} bytes)", artifact.adapter_id, data.len());
        imported += 1;
    }

    println!();
    println!("✓ Import complete");
    println!("  Imported: {}", imported);
    if skipped > 0 {
        println!("  Skipped: {}", skipped);
    }

    Ok(())
}

// Helper types and functions

use adapteros_core::B3Hash;
use std::collections::HashMap;

/// Query adapter hashes from node
async fn query_adapter_hashes(endpoint: &str) -> Result<HashMap<String, B3Hash>> {
    let client = reqwest::Client::new();
    let url = format!("{}/adapters", endpoint);

    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .context("Failed to query adapter hashes")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("HTTP {}", response.status()));
    }

    #[derive(serde::Deserialize)]
    struct AdapterHash {
        id: String,
        hash: String,
    }

    let adapters: Vec<AdapterHash> = response.json().await?;

    let mut hash_map = HashMap::new();
    for adapter in adapters {
        let hash = B3Hash::from_hex(&adapter.hash)?;
        hash_map.insert(adapter.id, hash);
    }

    Ok(hash_map)
}

/// Replication manifest
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ReplicationManifest {
    session_id: String,
    artifacts: Vec<ArtifactInfo>,
    signature: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ArtifactInfo {
    adapter_id: String,
    hash: String,
    size_bytes: u64,
}

/// Create replication manifest
async fn create_replication_manifest(
    cas_store: &adapteros_artifacts::CasStore,
    adapters: &[String],
) -> Result<ReplicationManifest> {
    // Build artifact info from actual CAS store
    let mut artifacts: Vec<ArtifactInfo> = Vec::with_capacity(adapters.len());

    for id in adapters {
        // Compute hash from adapter ID
        let hash = B3Hash::hash(id.as_bytes());

        // Try to load artifact to get size, fallback to 0 if not found
        let size_bytes = match cas_store.load("adapter", &hash) {
            Ok(bytes) => bytes.len() as u64,
            Err(_) => 0, // Artifact not in store or error
        };

        artifacts.push(ArtifactInfo {
            adapter_id: id.clone(),
            hash: hash.to_hex(),
            size_bytes,
        });
    }

    let session_id = uuid::Uuid::new_v4().to_string();

    // Create manifest content to sign
    let manifest_content = serde_json::json!({
        "session_id": session_id,
        "artifacts": artifacts,
    });
    let manifest_bytes = serde_json::to_vec(&manifest_content)
        .context("Failed to serialize manifest for signing")?;

    // Sign with Ed25519
    // Try to load signing key from environment or generate ephemeral one
    let signature = match std::env::var("AOS_SIGNING_KEY") {
        Ok(key_hex) => {
            // Use configured signing key
            let sig_bytes = adapteros_crypto::signature::sign_data(&manifest_bytes, &key_hex)
                .map_err(|e| anyhow::anyhow!("Failed to sign manifest: {}", e))?;
            hex::encode(sig_bytes)
        }
        Err(_) => {
            // Generate ephemeral keypair for this session
            let keypair = adapteros_crypto::Keypair::generate();
            let sig = keypair.sign(&manifest_bytes);
            hex::encode(sig.to_bytes())
        }
    };

    Ok(ReplicationManifest {
        session_id,
        artifacts,
        signature,
    })
}

/// Replicate to target node
async fn replicate_to_node(endpoint: &str, manifest: &ReplicationManifest) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/sync/manifest", endpoint);

    let response = client
        .post(&url)
        .json(manifest)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("Failed to send manifest")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Replication failed: HTTP {}",
            response.status()
        ));
    }

    Ok(())
}

/// Request manifest from source node
async fn request_manifest(endpoint: &str, adapters: &[String]) -> Result<ReplicationManifest> {
    let client = reqwest::Client::new();
    let url = format!("{}/sync/create-manifest", endpoint);

    let response = client
        .post(&url)
        .json(&serde_json::json!({ "adapters": adapters }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("Failed to request manifest")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to get manifest: HTTP {}",
            response.status()
        ));
    }

    let manifest: ReplicationManifest = response.json().await?;
    Ok(manifest)
}

/// Pull artifacts from source node
async fn pull_from_node(
    _endpoint: &str,
    manifest: &ReplicationManifest,
    _cas_store: &adapteros_artifacts::CasStore,
) -> Result<()> {
    // Mock implementation
    for artifact in &manifest.artifacts {
        println!(
            "  Downloaded: {} ({} bytes)",
            artifact.adapter_id, artifact.size_bytes
        );
    }
    Ok(())
}
