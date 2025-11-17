#!/usr/bin/env rust-script
//! Standalone script to verify snapshot determinism without full crate build
//!
//! ```cargo
//! [dependencies]
//! serde = { version = "1.0", features = ["derive"] }
//! serde_json = "1.0"
//! blake3 = "1.5"
//! chrono = { version = "0.4", features = ["serde"] }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct AdapterInfo {
    id: String,
    name: String,
    rank: u32,
    version: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct StackInfo {
    name: String,
    adapter_ids: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct PolicyInfo {
    name: String,
    rules: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct TenantStateSnapshot {
    tenant_id: String,
    adapters: Vec<AdapterInfo>,
    stacks: Vec<StackInfo>,
    router_policies: Vec<PolicyInfo>,
    plugin_configs: BTreeMap<String, serde_json::Value>,
    feature_flags: BTreeMap<String, bool>,
    configs: BTreeMap<String, serde_json::Value>,
    snapshot_timestamp: chrono::DateTime<chrono::Utc>,
}

impl TenantStateSnapshot {
    fn compute_hash(&self) -> String {
        // Clone and enforce canonical ordering
        let mut canonical = self.clone();
        canonical.adapters.sort_by(|a, b| a.id.cmp(&b.id));
        canonical.stacks.sort_by(|a, b| a.name.cmp(&b.name));
        canonical.router_policies.sort_by(|a, b| a.name.cmp(&b.name));

        // Sort adapter_ids within each stack for determinism
        for stack in &mut canonical.stacks {
            stack.adapter_ids.sort();
        }

        // Sort rules within each policy for determinism
        for policy in &mut canonical.router_policies {
            policy.rules.sort();
        }

        // Serialize to canonical JSON (BTreeMap ensures sorted keys)
        let json = serde_json::to_string(&canonical).expect("Serialization failed");
        let hash = blake3::hash(json.as_bytes());
        hash.to_hex().to_string()
    }
}

fn main() {
    println!("Testing deterministic snapshot hashing...\n");

    let timestamp = chrono::Utc::now();

    // Create snapshot with unsorted data
    let snapshot1 = TenantStateSnapshot {
        tenant_id: "tenant-1".to_string(),
        adapters: vec![
            AdapterInfo {
                id: "c".to_string(),
                name: "C".to_string(),
                rank: 1,
                version: "1.0".to_string(),
            },
            AdapterInfo {
                id: "a".to_string(),
                name: "A".to_string(),
                rank: 2,
                version: "1.0".to_string(),
            },
            AdapterInfo {
                id: "b".to_string(),
                name: "B".to_string(),
                rank: 3,
                version: "1.0".to_string(),
            },
        ],
        stacks: vec![StackInfo {
            name: "stack-1".to_string(),
            adapter_ids: vec!["c".to_string(), "a".to_string(), "b".to_string()],
        }],
        router_policies: vec![PolicyInfo {
            name: "policy-1".to_string(),
            rules: vec!["rule-c".to_string(), "rule-a".to_string()],
        }],
        plugin_configs: BTreeMap::new(),
        feature_flags: BTreeMap::new(),
        configs: BTreeMap::new(),
        snapshot_timestamp: timestamp,
    };

    // Create identical snapshot with different order
    let snapshot2 = TenantStateSnapshot {
        tenant_id: "tenant-1".to_string(),
        adapters: vec![
            AdapterInfo {
                id: "a".to_string(),
                name: "A".to_string(),
                rank: 2,
                version: "1.0".to_string(),
            },
            AdapterInfo {
                id: "b".to_string(),
                name: "B".to_string(),
                rank: 3,
                version: "1.0".to_string(),
            },
            AdapterInfo {
                id: "c".to_string(),
                name: "C".to_string(),
                rank: 1,
                version: "1.0".to_string(),
            },
        ],
        stacks: vec![StackInfo {
            name: "stack-1".to_string(),
            adapter_ids: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        }],
        router_policies: vec![PolicyInfo {
            name: "policy-1".to_string(),
            rules: vec!["rule-a".to_string(), "rule-c".to_string()],
        }],
        plugin_configs: BTreeMap::new(),
        feature_flags: BTreeMap::new(),
        configs: BTreeMap::new(),
        snapshot_timestamp: timestamp,
    };

    // Compute hashes
    let hash1 = snapshot1.compute_hash();
    let hash2 = snapshot2.compute_hash();

    println!("Snapshot 1 hash: {}", hash1);
    println!("Snapshot 2 hash: {}", hash2);
    println!();

    if hash1 == hash2 {
        println!("✓ SUCCESS: Identical hashes despite different input order");
        println!("✓ Determinism verified: canonical ordering works correctly");
        std::process::exit(0);
    } else {
        println!("✗ FAILURE: Hashes differ despite identical content");
        println!("✗ Determinism violated: canonical ordering is broken");
        std::process::exit(1);
    }
}
