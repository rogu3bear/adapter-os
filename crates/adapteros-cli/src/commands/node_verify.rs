//! Node verify command - check cross-node determinism

use adapteros_core::B3Hash;
use adapteros_db::Db;
use anyhow::{Context, Result};
use std::collections::HashMap;

/// Verify determinism across nodes
pub async fn run(all: bool, node_ids: Option<Vec<String>>) -> Result<()> {
    let db = Db::connect_env().await?;

    println!("Cross-Node Verification");
    println!("=======================\n");

    // Determine which nodes to verify
    let nodes = if all || node_ids.is_none() {
        db.list_nodes().await?
    } else if let Some(ids) = node_ids {
        let mut selected = Vec::new();
        for id in ids {
            if let Some(node) = db.get_node(&id).await? {
                selected.push(node);
            } else {
                println!("⚠ Node not found: {}", id);
            }
        }
        selected
    } else {
        Vec::new()
    };

    if nodes.is_empty() {
        return Err(anyhow::anyhow!("No nodes to verify"));
    }

    println!("Verifying {} node(s)...\n", nodes.len());

    // Collect hashes from each node
    let mut hash_map: HashMap<String, Vec<(String, B3Hash)>> = HashMap::new();
    let mut errors = Vec::new();

    for node in &nodes {
        print!("  Querying {}... ", node.hostname);

        match query_node_hashes(&node.agent_endpoint).await {
            Ok(hashes) => {
                println!("✓ {} hashes", hashes.len());

                for (component, hash) in hashes {
                    hash_map
                        .entry(component)
                        .or_default()
                        .push((node.hostname.clone(), hash));
                }
            }
            Err(e) => {
                println!("✗ {}", e);
                errors.push((node.hostname.clone(), e.to_string()));
            }
        }
    }

    println!();

    // Analyze consistency
    let mut all_consistent = true;

    for (component, node_hashes) in &hash_map {
        // Check if all hashes match
        let unique_hashes: std::collections::HashSet<_> =
            node_hashes.iter().map(|(_, h)| h).collect();

        let consistent = unique_hashes.len() == 1;
        let symbol = if consistent { "✓" } else { "✗" };

        if consistent {
            let hash = node_hashes[0].1.to_hex();
            let short_hash = &hash[..12];
            println!(
                "  {} {}: b3:{}... ({}/{} nodes)",
                symbol,
                component,
                short_hash,
                node_hashes.len(),
                nodes.len()
            );
        } else {
            println!(
                "  {} {}: MISMATCH ({}/{} nodes)",
                symbol,
                component,
                node_hashes.len(),
                nodes.len()
            );

            // Show which nodes have which hashes
            for (node, hash) in node_hashes {
                let short_hash = &hash.to_hex()[..12];
                println!("      {} → b3:{}...", node, short_hash);
            }

            all_consistent = false;
        }
    }

    // Report errors
    if !errors.is_empty() {
        println!("\nErrors:");
        for (node, error) in errors {
            println!("  ✗ {}: {}", node, error);
        }
        all_consistent = false;
    }

    println!();

    if all_consistent {
        println!("✓ All nodes consistent");
        Ok(())
    } else {
        Err(anyhow::anyhow!("Hash mismatches detected across nodes"))
    }
}

/// Component hashes from node
type ComponentHashes = Vec<(String, B3Hash)>;

/// Query node for hashes
async fn query_node_hashes(endpoint: &str) -> Result<ComponentHashes> {
    let client = reqwest::Client::new();
    let url = format!("{}/hashes", endpoint);

    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .context("Failed to connect to node agent")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("HTTP {}", response.status()));
    }

    #[derive(serde::Deserialize)]
    struct HashResponse {
        component: String,
        hash: String,
    }

    let hash_responses: Vec<HashResponse> = response.json().await?;

    let mut hashes = Vec::new();
    for resp in hash_responses {
        let hash = B3Hash::from_hex(&resp.hash).context("Invalid hash from node")?;
        hashes.push((resp.component, hash));
    }

    Ok(hashes)
}
