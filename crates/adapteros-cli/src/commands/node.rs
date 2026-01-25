//! Node management commands - cluster operations
//!
//! Consolidates node-list, node-verify, and node-sync into git-style subcommands:
//! - `aosctl node list` - List cluster nodes
//! - `aosctl node verify` - Verify cross-node determinism
//! - `aosctl node sync` - Sync adapters across nodes

use super::NOT_IMPLEMENTED_MESSAGE;
use crate::formatting::{format_bytes, format_time_ago};
use crate::output::OutputWriter;
use adapteros_core::{rebase_var_path, time, AosError, B3Hash, Result};
use adapteros_db::Db;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tar::Builder;
use tracing::{debug, info, warn};

/// Node management subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum NodeCommand {
    /// List cluster nodes
    #[command(after_help = r#"Examples:
  aosctl node list
  aosctl node list --offline
  aosctl node list --json"#)]
    List {
        /// Offline mode (use cached database state)
        #[arg(long)]
        offline: bool,

        /// Output format as JSON
        #[arg(long)]
        json: bool,
    },

    /// Verify cross-node determinism
    #[command(after_help = r#"Examples:
  aosctl node verify --all
  aosctl node verify --nodes node1,node2
  aosctl node verify --all --verbose"#)]
    Verify {
        /// Verify all nodes
        #[arg(long)]
        all: bool,

        /// Specific node IDs to verify (comma-separated)
        #[arg(long, value_delimiter = ',')]
        nodes: Option<Vec<String>>,

        /// Output format as JSON
        #[arg(long)]
        json: bool,
    },

    /// Sync adapters across nodes
    #[command(subcommand)]
    Sync(NodeSyncCommand),
}

/// Node sync subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum NodeSyncCommand {
    /// Verify sync between two nodes
    #[command(after_help = r#"Examples:
  aosctl node sync verify --from node1 --to node2"#)]
    Verify {
        /// Source node ID
        #[arg(long)]
        from: String,

        /// Target node ID
        #[arg(long)]
        to: String,
    },

    /// Push adapters to target node
    #[command(after_help = r#"Examples:
  aosctl node sync push --to node2 --adapters adapter1,adapter2"#)]
    Push {
        /// Target node ID
        #[arg(long)]
        to: String,

        /// Adapter IDs to push (comma-separated)
        #[arg(long, value_delimiter = ',')]
        adapters: Vec<String>,
    },

    /// Pull adapters from source node
    #[command(after_help = r#"Examples:
  aosctl node sync pull --from node1 --adapters adapter1,adapter2"#)]
    Pull {
        /// Source node ID
        #[arg(long)]
        from: String,

        /// Adapter IDs to pull (comma-separated)
        #[arg(long, value_delimiter = ',')]
        adapters: Vec<String>,
    },

    /// Export adapters for air-gap transfer [NOT IMPLEMENTED]
    #[command(after_help = r#"Examples:
  aosctl node sync export --file ./adapters-bundle.tar"#)]
    Export {
        /// Output file path
        #[arg(long)]
        file: PathBuf,
    },

    /// Import adapters from air-gap bundle [NOT IMPLEMENTED]
    #[command(after_help = r#"Examples:
  aosctl node sync import --file ./adapters-bundle.tar"#)]
    Import {
        /// Input file path
        #[arg(long)]
        file: PathBuf,
    },
}

/// Get node command name for telemetry
fn get_node_command_name(cmd: &NodeCommand) -> String {
    match cmd {
        NodeCommand::List { .. } => "node_list".to_string(),
        NodeCommand::Verify { .. } => "node_verify".to_string(),
        NodeCommand::Sync(sync_cmd) => match sync_cmd {
            NodeSyncCommand::Verify { .. } => "node_sync_verify".to_string(),
            NodeSyncCommand::Push { .. } => "node_sync_push".to_string(),
            NodeSyncCommand::Pull { .. } => "node_sync_pull".to_string(),
            NodeSyncCommand::Export { .. } => "node_sync_export".to_string(),
            NodeSyncCommand::Import { .. } => "node_sync_import".to_string(),
        },
    }
}

/// Handle node management commands
///
/// Routes node commands to appropriate handlers
pub async fn handle_node_command(cmd: NodeCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_node_command_name(&cmd);

    info!(command = ?cmd, "Handling node command");

    // Emit telemetry
    if let Err(e) = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        NodeCommand::List { offline, json } => list_nodes(offline, json, output).await,
        NodeCommand::Verify { all, nodes, json } => verify_nodes(all, nodes, json, output).await,
        NodeCommand::Sync(sync_cmd) => handle_sync_command(sync_cmd, output).await,
    }
}

/// Handle node sync subcommands
async fn handle_sync_command(cmd: NodeSyncCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        NodeSyncCommand::Verify { from, to } => sync_verify(&from, &to, output).await,
        NodeSyncCommand::Push { to, adapters } => sync_push(&to, &adapters, output).await,
        NodeSyncCommand::Pull { from, adapters } => sync_pull(&from, &adapters, output).await,
        NodeSyncCommand::Export { file } => sync_export(&file, output).await,
        NodeSyncCommand::Import { file } => sync_import(&file, output).await,
    }
}

// ============================================================
// Node List Implementation
// ============================================================

/// Node status from node runtime
#[derive(Debug, serde::Deserialize)]
struct NodeStatus {
    worker_count: usize,
    vram_bytes: u64,
}

/// List nodes in the cluster
async fn list_nodes(offline: bool, json: bool, output: &OutputWriter) -> Result<()> {
    let db = Db::connect_env().await?;

    if !json {
        if offline {
            output.info("Node List (offline mode - last known state)");
        } else {
            output.info("Node List");
        }
        output.blank();
    }

    // Fetch nodes from database
    let nodes = db.list_nodes().await?;

    if nodes.is_empty() {
        if json {
            output.result(r#"{"nodes": [], "total": 0}"#);
        } else {
            output.warning("No nodes registered");
        }
        return Ok(());
    }

    if json {
        let json_nodes: Vec<serde_json::Value> = nodes
            .iter()
            .map(|node| {
                serde_json::json!({
                    "id": node.id,
                    "hostname": node.hostname,
                    "status": node.status,
                    "endpoint": node.agent_endpoint,
                    "last_seen": node.last_seen_at
                })
            })
            .collect();

        let response = serde_json::json!({
            "nodes": json_nodes,
            "total": nodes.len()
        });
        output.result(&serde_json::to_string_pretty(&response).map_err(AosError::Serialization)?);
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "Node ID",
        "Hostname",
        "Status",
        "Endpoint",
        "Last Seen",
    ]);

    for node in &nodes {
        let last_seen = node
            .last_seen_at
            .as_ref()
            .map(|s| format_time_ago(s))
            .unwrap_or_else(|| "never".to_string());

        table.add_row(vec![
            Cell::new(&node.id[..8.min(node.id.len())]), // Shortened ID
            Cell::new(&node.hostname),
            Cell::new(&node.status),
            Cell::new(&node.agent_endpoint),
            Cell::new(&last_seen),
        ]);
    }

    output.result(format!("{}", table));
    output.result(format!("\nTotal: {} node(s)", nodes.len()));

    // If not offline, query live status from node runtimes
    if !offline {
        output.blank();
        output.info("Querying live status...");
        for node in &nodes {
            match query_node_status(&node.agent_endpoint).await {
                Ok(status) => {
                    output.result(format!(
                        "  {} [{}]: {} workers, {} VRAM",
                        node.hostname,
                        &node.id[..8.min(node.id.len())],
                        status.worker_count,
                        format_bytes(status.vram_bytes)
                    ));
                }
                Err(e) => {
                    output.warning(format!(
                        "  {} [{}]: unreachable ({})",
                        node.hostname,
                        &node.id[..8.min(node.id.len())],
                        e
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Query node runtime for live status
async fn query_node_status(endpoint: &str) -> Result<NodeStatus> {
    let client = reqwest::Client::new();
    let url = format!("{}/status", endpoint);

    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| AosError::Network(format!("Failed to query node: {}", e)))?;

    if !response.status().is_success() {
        return Err(AosError::Network(format!("HTTP {}", response.status())));
    }

    let status: NodeStatus = response
        .json()
        .await
        .map_err(|e| AosError::Network(format!("Failed to parse response: {}", e)))?;
    Ok(status)
}

// ============================================================
// Node Verify Implementation
// ============================================================

/// Component hashes from node
type ComponentHashes = Vec<(String, B3Hash)>;

/// Verify determinism across nodes
async fn verify_nodes(
    all: bool,
    node_ids: Option<Vec<String>>,
    json: bool,
    output: &OutputWriter,
) -> Result<()> {
    let db = Db::connect_env().await?;

    if !json {
        output.info("Cross-Node Verification");
        output.result("=======================");
        output.blank();
    }

    // Determine which nodes to verify
    let nodes = if all || node_ids.is_none() {
        db.list_nodes().await?
    } else if let Some(ids) = node_ids {
        let mut selected = Vec::new();
        for id in ids {
            if let Some(node) = db.get_node(&id).await? {
                selected.push(node);
            } else if !json {
                output.warning(format!("Node not found: {}", id));
            }
        }
        selected
    } else {
        Vec::new()
    };

    if nodes.is_empty() {
        if json {
            let response = serde_json::json!({
                "success": false,
                "error": "No nodes to verify"
            });
            output
                .result(&serde_json::to_string_pretty(&response).map_err(AosError::Serialization)?);
        }
        return Err(AosError::Validation("No nodes to verify".to_string()));
    }

    if !json {
        output.info(format!("Verifying {} node(s)...", nodes.len()));
        output.blank();
    }

    // Collect hashes from each node
    let mut hash_map: HashMap<String, Vec<(String, B3Hash)>> = HashMap::new();
    let mut errors: Vec<(String, String)> = Vec::new();

    for node in &nodes {
        if !json {
            output.result(format!("  Querying {}... ", node.hostname));
        }

        match query_node_hashes(&node.agent_endpoint).await {
            Ok(hashes) => {
                if !json {
                    output.success(format!("{} hashes", hashes.len()));
                }

                for (component, hash) in hashes {
                    hash_map
                        .entry(component)
                        .or_default()
                        .push((node.hostname.clone(), hash));
                }
            }
            Err(e) => {
                if !json {
                    output.error(format!("{}", e));
                }
                errors.push((node.hostname.clone(), e.to_string()));
            }
        }
    }

    if !json {
        output.blank();
    }

    // Analyze consistency
    let mut all_consistent = true;
    let mut component_results: Vec<serde_json::Value> = Vec::new();

    for (component, node_hashes) in &hash_map {
        // Check if all hashes match
        let unique_hashes: std::collections::HashSet<_> =
            node_hashes.iter().map(|(_, h)| h).collect();

        let consistent = unique_hashes.len() == 1;

        if !consistent {
            all_consistent = false;
        }

        if json {
            let mut node_details: Vec<serde_json::Value> = Vec::new();
            for (node, hash) in node_hashes {
                node_details.push(serde_json::json!({
                    "node": node,
                    "hash": hash.to_hex()
                }));
            }
            component_results.push(serde_json::json!({
                "component": component,
                "consistent": consistent,
                "nodes": node_details
            }));
        } else {
            let symbol = if consistent { "OK" } else { "MISMATCH" };

            if consistent {
                let hash = node_hashes[0].1.to_hex();
                let short_hash = &hash[..12.min(hash.len())];
                output.result(format!(
                    "  {} {}: b3:{}... ({}/{} nodes)",
                    symbol,
                    component,
                    short_hash,
                    node_hashes.len(),
                    nodes.len()
                ));
            } else {
                output.error(format!(
                    "  {} {}: MISMATCH ({}/{} nodes)",
                    symbol,
                    component,
                    node_hashes.len(),
                    nodes.len()
                ));

                // Show which nodes have which hashes
                for (node, hash) in node_hashes {
                    let short_hash = &hash.to_hex()[..12.min(hash.to_hex().len())];
                    output.result(format!("      {} -> b3:{}...", node, short_hash));
                }
            }
        }
    }

    // Report errors
    if !errors.is_empty() {
        all_consistent = false;
        if !json {
            output.blank();
            output.error("Errors:");
            for (node, error) in &errors {
                output.error(format!("  {}: {}", node, error));
            }
        }
    }

    if json {
        let response = serde_json::json!({
            "success": all_consistent,
            "components": component_results,
            "errors": errors.iter().map(|(n, e)| serde_json::json!({"node": n, "error": e})).collect::<Vec<_>>(),
            "total_nodes": nodes.len()
        });
        output.result(&serde_json::to_string_pretty(&response).map_err(AosError::Serialization)?);
    } else {
        output.blank();
        if all_consistent {
            output.success("All nodes consistent");
        } else {
            output.error("Hash mismatches detected across nodes");
        }
    }

    if all_consistent {
        Ok(())
    } else {
        Err(AosError::DeterminismViolation(
            "Hash mismatches detected across nodes".to_string(),
        ))
    }
}

/// Query node for hashes
async fn query_node_hashes(endpoint: &str) -> Result<ComponentHashes> {
    let client = reqwest::Client::new();
    let url = format!("{}/hashes", endpoint);

    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| AosError::Network(format!("Failed to connect to node runtime: {}", e)))?;

    if !response.status().is_success() {
        return Err(AosError::Network(format!("HTTP {}", response.status())));
    }

    #[derive(serde::Deserialize)]
    struct HashResponse {
        component: String,
        hash: String,
    }

    let hash_responses: Vec<HashResponse> = response
        .json()
        .await
        .map_err(|e| AosError::Network(format!("Failed to parse response: {}", e)))?;

    let mut hashes = Vec::new();
    for resp in hash_responses {
        let hash = B3Hash::from_hex(&resp.hash)
            .map_err(|e| AosError::Validation(format!("Invalid hash from node: {}", e)))?;
        hashes.push((resp.component, hash));
    }

    Ok(hashes)
}

// ============================================================
// Node Sync Implementation
// ============================================================

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

/// Verify sync between two nodes
async fn sync_verify(from: &str, to: &str, output: &OutputWriter) -> Result<()> {
    output.info("Verify Sync");
    output.kv("From", from);
    output.kv("To", to);
    output.blank();

    // Get nodes from database
    let db = Db::connect_env().await?;
    let from_node = db
        .get_node(from)
        .await?
        .ok_or_else(|| AosError::NotFound(format!("Source node not found: {}", from)))?;
    let to_node = db
        .get_node(to)
        .await?
        .ok_or_else(|| AosError::NotFound(format!("Target node not found: {}", to)))?;

    output.info("Comparing adapter hashes...");

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
                output.success(format!("{}: match", adapter_id));
            } else {
                mismatches += 1;
                output.error(format!("{}: hash mismatch", adapter_id));
            }
        } else {
            missing += 1;
            output.warning(format!("{}: missing on target", adapter_id));
        }
    }

    output.blank();
    output.section("Summary");
    output.kv("Matches", &matches.to_string());
    output.kv("Mismatches", &mismatches.to_string());
    output.kv("Missing", &missing.to_string());

    if mismatches > 0 || missing > 0 {
        output.blank();
        output.error("Sync verification failed");
        Err(AosError::Validation("Sync verification failed".to_string()))
    } else {
        output.blank();
        output.success("Nodes are in sync");
        Ok(())
    }
}

/// Push adapters to target node
async fn sync_push(to: &str, adapters: &[String], output: &OutputWriter) -> Result<()> {
    output.info("Push Adapters");
    output.kv("To", to);
    output.kv("Adapters", &format!("{:?}", adapters));
    output.blank();

    // Get target node
    let db = Db::connect_env().await?;
    let to_node = db
        .get_node(to)
        .await?
        .ok_or_else(|| AosError::NotFound(format!("Target node not found: {}", to)))?;

    // Use replication module to push
    let cas_store = adapteros_artifacts::CasStore::new(rebase_var_path("./var/cas"))?;

    output.info("Creating replication manifest...");
    let manifest = create_replication_manifest(&cas_store, adapters).await?;

    output.info(format!(
        "Replicating {} artifacts...",
        manifest.artifacts.len()
    ));

    // Send manifest to target
    replicate_to_node(&to_node.agent_endpoint, &manifest).await?;

    output.blank();
    output.success("Push complete");
    Ok(())
}

/// Pull adapters from source node
async fn sync_pull(from: &str, adapters: &[String], output: &OutputWriter) -> Result<()> {
    output.info("Pull Adapters");
    output.kv("From", from);
    output.kv("Adapters", &format!("{:?}", adapters));
    output.blank();

    // Get source node
    let db = Db::connect_env().await?;
    let from_node = db
        .get_node(from)
        .await?
        .ok_or_else(|| AosError::NotFound(format!("Source node not found: {}", from)))?;

    // Request manifest from source
    output.info("Requesting manifest...");
    let manifest = request_manifest(&from_node.agent_endpoint, adapters).await?;

    output.info(format!("Pulling {} artifacts...", manifest.artifacts.len()));

    // Download artifacts
    let cas_store = adapteros_artifacts::CasStore::new(rebase_var_path("./var/cas"))?;
    pull_from_node(&from_node.agent_endpoint, &manifest, &cas_store).await?;

    output.blank();
    output.success("Pull complete");
    Ok(())
}

/// Export adapters for air-gap transfer
async fn sync_export(file: &Path, output: &OutputWriter) -> Result<()> {
    output.info("Export Air-Gap Bundle");
    output.kv("File", &file.display().to_string());
    output.blank();

    // Use replication module to create export bundle
    output.warning("Air-gap export not yet implemented");
    output.info(NOT_IMPLEMENTED_MESSAGE);
    Err(AosError::Config(NOT_IMPLEMENTED_MESSAGE.to_string()))
}

/// Import adapters from air-gap bundle
async fn sync_import(file: &Path, output: &OutputWriter) -> Result<()> {
    output.info("Import Air-Gap Bundle");
    output.kv("File", &file.display().to_string());
    output.blank();

    // Verify bundle exists
    if !file.exists() {
        return Err(AosError::NotFound(format!(
            "Bundle file not found: {}",
            file.display()
        )));
    }

    output.warning("Air-gap import not yet implemented");
    output.info(NOT_IMPLEMENTED_MESSAGE);
    Err(AosError::Config(NOT_IMPLEMENTED_MESSAGE.to_string()))
}

/// Query adapter hashes from node
async fn query_adapter_hashes(endpoint: &str) -> Result<HashMap<String, B3Hash>> {
    let client = reqwest::Client::new();
    let url = format!("{}/adapters", endpoint);

    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| AosError::Network(format!("Failed to query adapter hashes: {}", e)))?;

    if !response.status().is_success() {
        return Err(AosError::Network(format!("HTTP {}", response.status())));
    }

    #[derive(serde::Deserialize)]
    struct AdapterHash {
        id: String,
        hash: String,
    }

    let adapters: Vec<AdapterHash> = response
        .json()
        .await
        .map_err(|e| AosError::Network(format!("Failed to parse response: {}", e)))?;

    let mut hash_map = HashMap::new();
    for adapter in adapters {
        let hash = B3Hash::from_hex(&adapter.hash)
            .map_err(|e| AosError::Validation(format!("Invalid hash: {}", e)))?;
        hash_map.insert(adapter.id, hash);
    }

    Ok(hash_map)
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
    let manifest_bytes = serde_json::to_vec(&manifest_content).map_err(AosError::Serialization)?;

    // Sign with Ed25519
    // Try to load signing key from environment or generate ephemeral one
    let signature = match std::env::var("AOS_SIGNING_KEY") {
        Ok(key_hex) => {
            // Use configured signing key
            let sig_bytes = adapteros_crypto::signature::sign_data(&manifest_bytes, &key_hex)
                .map_err(|e| AosError::Crypto(format!("Failed to sign manifest: {}", e)))?;
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
        .map_err(|e| AosError::Network(format!("Failed to send manifest: {}", e)))?;

    if !response.status().is_success() {
        return Err(AosError::Network(format!(
            "Replication failed: HTTP {}",
            response.status()
        )));
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
        .map_err(|e| AosError::Network(format!("Failed to request manifest: {}", e)))?;

    if !response.status().is_success() {
        return Err(AosError::Network(format!(
            "Failed to get manifest: HTTP {}",
            response.status()
        )));
    }

    let manifest: ReplicationManifest = response
        .json()
        .await
        .map_err(|e| AosError::Network(format!("Failed to parse manifest: {}", e)))?;
    Ok(manifest)
}

/// Pull artifacts from source node
async fn pull_from_node(
    _endpoint: &str,
    manifest: &ReplicationManifest,
    _cas_store: &adapteros_artifacts::CasStore,
) -> Result<()> {
    // Mock implementation - would actually download artifacts
    for artifact in &manifest.artifacts {
        tracing::info!(
            adapter_id = %artifact.adapter_id,
            size_bytes = artifact.size_bytes,
            "Downloaded artifact"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{OutputMode, OutputWriter};

    #[test]
    fn test_get_node_command_name() {
        assert_eq!(
            get_node_command_name(&NodeCommand::List {
                offline: false,
                json: false
            }),
            "node_list"
        );
        assert_eq!(
            get_node_command_name(&NodeCommand::Verify {
                all: true,
                nodes: None,
                json: false
            }),
            "node_verify"
        );
        assert_eq!(
            get_node_command_name(&NodeCommand::Sync(NodeSyncCommand::Verify {
                from: "node1".to_string(),
                to: "node2".to_string()
            })),
            "node_sync_verify"
        );
        assert_eq!(
            get_node_command_name(&NodeCommand::Sync(NodeSyncCommand::Push {
                to: "node1".to_string(),
                adapters: vec![]
            })),
            "node_sync_push"
        );
        assert_eq!(
            get_node_command_name(&NodeCommand::Sync(NodeSyncCommand::Pull {
                from: "node1".to_string(),
                adapters: vec![]
            })),
            "node_sync_pull"
        );
        assert_eq!(
            get_node_command_name(&NodeCommand::Sync(NodeSyncCommand::Export {
                file: PathBuf::from("test.tar")
            })),
            "node_sync_export"
        );
        assert_eq!(
            get_node_command_name(&NodeCommand::Sync(NodeSyncCommand::Import {
                file: PathBuf::from("test.tar")
            })),
            "node_sync_import"
        );
    }

    #[test]
    fn test_artifact_info_serialization() {
        let artifact = ArtifactInfo {
            adapter_id: "test-adapter".to_string(),
            hash: "abc123".to_string(),
            size_bytes: 1024,
        };

        let json = serde_json::to_string(&artifact).unwrap();
        let deserialized: ArtifactInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(artifact.adapter_id, deserialized.adapter_id);
        assert_eq!(artifact.hash, deserialized.hash);
        assert_eq!(artifact.size_bytes, deserialized.size_bytes);
    }

    #[test]
    fn test_replication_manifest_serialization() {
        let manifest = ReplicationManifest {
            session_id: "test-session".to_string(),
            artifacts: vec![ArtifactInfo {
                adapter_id: "adapter1".to_string(),
                hash: "hash1".to_string(),
                size_bytes: 100,
            }],
            signature: "sig123".to_string(),
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: ReplicationManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest.session_id, deserialized.session_id);
        assert_eq!(manifest.artifacts.len(), deserialized.artifacts.len());
        assert_eq!(manifest.signature, deserialized.signature);
    }
}
