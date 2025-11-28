use crate::Db;
use adapteros_core::{AosError, B3Hash, Result, IndexSnapshot, GraphNode, StackInfo};
use serde_json::Value;
use sqlx::Row;
use std::collections::BTreeMap;
use tracing::{debug, warn};

impl Db {
    pub async fn store_index_hash(&self, tenant_id: &str, index_type: &str, hash: &B3Hash) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO index_hashes (tenant_id, index_type, hash, updated_at) VALUES (?, ?, ?, datetime('now'))")
            .bind(tenant_id)
            .bind(index_type)
            .bind(hash.to_hex())
            .execute(&*self.pool())
            .await?;
        Ok(())
    }

    pub async fn get_index_hash(&self, tenant_id: &str, index_type: &str) -> Result<Option<B3Hash>> {
        let hash_str = sqlx::query("SELECT hash FROM index_hashes WHERE tenant_id = ? AND index_type = ? ORDER BY updated_at DESC LIMIT 1")
            .bind(tenant_id)
            .bind(index_type)
            .fetch_optional(&*self.pool())
            .await?
            .map(|row| row.get::<String, _>(0));
        Ok(hash_str.and_then(|s| B3Hash::from_hex(&s).ok()))
    }

    pub async fn verify_index(&self, tenant_id: &str, index_type: &str) -> Result<bool> {
        // First, verify tenant snapshot as base
        if let Some(stored_ts_hash) = self.get_tenant_snapshot_hash(tenant_id).await? {
            // Rebuild tenant snapshot from current DB state (placeholder: would query all relevant tables)
            // For now, assume a function to rebuild
            let rebuilt_ts = rebuild_tenant_snapshot_from_db(tenant_id, self).await?; // Implement separately
            let computed_ts = rebuilt_ts.compute_hash();
            if stored_ts_hash != computed_ts {
                warn!("Tenant snapshot hash mismatch for {}", tenant_id);
                return Ok(false);
            }
        }

        let stored_hash = self.get_index_hash(tenant_id, index_type).await?;
        if let Some(stored) = stored_hash {
            let rebuilt = build_index_snapshot(tenant_id, index_type, self).await?;
            let computed = rebuilt.compute_hash();
            Ok(stored == computed)
        } else {
            // No stored hash, rebuild and store implicitly ok
            Ok(true)
        }
    }

    pub async fn rebuild_all_indexes(&self, tenant_id: &str) -> Result<()> {
        let types = vec!["adapter_graph", "stacks", "router_table", "telemetry_secondary"];
        for typ in types {
            match build_index_snapshot(tenant_id, typ, self).await {
                Ok(snapshot) => {
                    let hash = snapshot.compute_hash();
                    if let Err(e) = self.store_index_hash(tenant_id, typ, &hash).await {
                        warn!("Failed to store index hash for {}: {}", typ, e);
                    }
                }
                Err(e) => {
                    warn!("Failed to rebuild index {} for tenant {}: {}", typ, tenant_id, e);
                    // Continue to rebuild others, or return Err(e) for strict
                }
            }
        }
        Ok(())
    }
}

// Full implementation of build_index_snapshot
pub async fn build_index_snapshot(tenant_id: &str, index_type: &str, db: &Db) -> Result<IndexSnapshot> {
    match index_type {
        "adapter_graph" => {
            let adapters = db.list_adapters(tenant_id).await?;

            // Build edges from adapter_stacks relationships
            // Adapters in the same stack are considered connected
            let stacks_rows = sqlx::query(
                "SELECT adapter_ids_json FROM adapter_stacks WHERE tenant_id = ?"
            )
            .bind(tenant_id)
            .fetch_all(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to query adapter stacks for edges: {}", e)))?;

            // Build edge map from stack relationships
            let mut edge_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for row in stacks_rows {
                let adapter_ids_json: String = row.get("adapter_ids_json");
                if let Ok(adapter_ids) = serde_json::from_str::<Vec<String>>(&adapter_ids_json) {
                    // Create bidirectional edges between all adapters in the same stack
                    for i in 0..adapter_ids.len() {
                        for j in (i + 1)..adapter_ids.len() {
                            edge_map.entry(adapter_ids[i].clone()).or_default().push(adapter_ids[j].clone());
                            edge_map.entry(adapter_ids[j].clone()).or_default().push(adapter_ids[i].clone());
                        }
                    }
                }
            }

            let mut nodes: Vec<GraphNode> = adapters.into_iter().map(|a| {
                let mut edges = edge_map.remove(&a.id).unwrap_or_default();
                edges.sort();
                edges.dedup();
                GraphNode {
                    id: a.id.clone(),
                    edges,
                }
            }).collect();
            nodes.sort_by(|n1, n2| n1.id.cmp(&n2.id));
            Ok(IndexSnapshot::AdapterGraph(nodes))
        },
        "stacks" => {
            // Assume db.list_adapter_stacks(tenant_id) exists from migration 0064
            let stacks_db = db.list_adapter_stacks(tenant_id).await?; // Placeholder: implement if needed
            let mut stacks: Vec<StackInfo> = stacks_db.into_iter().map(|s| StackInfo {
                name: s.name,
                adapter_ids: {
                    let mut ids = s.adapter_ids;
                    ids.sort(); // Canonical sort
                    ids
                },
            }).collect();
            stacks.sort_by(|s1, s2| s1.name.cmp(&s2.name));
            Ok(IndexSnapshot::AdapterStacks(stacks))
        },
        "router_table" => {
            // Query router priors from routing_decisions table
            // Aggregate average entropy per adapter as prior weight
            let rows = sqlx::query(
                "SELECT selected_adapter_ids, AVG(entropy) as avg_entropy
                 FROM routing_decisions
                 WHERE tenant_id = ? AND selected_adapter_ids IS NOT NULL
                 GROUP BY selected_adapter_ids
                 ORDER BY selected_adapter_ids"
            )
            .bind(tenant_id)
            .fetch_all(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to query routing decisions: {}", e)))?;

            let mut priors: BTreeMap<String, f64> = BTreeMap::new();
            for row in rows {
                let adapter_ids: String = row.get("selected_adapter_ids");
                let avg_entropy: f64 = row.get("avg_entropy");
                // Split comma-separated adapter IDs and assign entropy as prior
                for adapter_id in adapter_ids.split(',').map(|s| s.trim()) {
                    if !adapter_id.is_empty() {
                        // Use entry to accumulate if adapter appears in multiple selections
                        priors.entry(adapter_id.to_string())
                            .and_modify(|e| *e = (*e + avg_entropy) / 2.0)
                            .or_insert(avg_entropy);
                    }
                }
            }
            debug!(tenant_id = %tenant_id, num_priors = priors.len(), "Built router table priors");
            Ok(IndexSnapshot::RouterTable(priors))
        },
        "telemetry_secondary" => {
            // Query recent activity events grouped by user_id
            let rows = sqlx::query(
                "SELECT user_id, GROUP_CONCAT(event_type) as event_types
                 FROM activity_events
                 WHERE tenant_id = ?
                 GROUP BY user_id
                 ORDER BY user_id"
            )
            .bind(tenant_id)
            .fetch_all(db.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to query activity events: {}", e)))?;

            let mut secondary: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for row in rows {
                let user_id: String = row.get("user_id");
                let event_types: String = row.get("event_types");
                let mut events: Vec<String> = event_types
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                events.sort();
                secondary.insert(user_id, events);
            }
            debug!(tenant_id = %tenant_id, num_users = secondary.len(), "Built telemetry secondary index");
            Ok(IndexSnapshot::TelemetrySecondary(secondary))
        },
        _ => Err(AosError::Validation(format!("Unsupported index type: {}", index_type)).into()),
    }
}

// Local structs for index building
#[derive(Debug, Clone)]
struct AdapterInfo {
    id: String,
}

#[derive(Debug, Clone)]
struct StackDb {
    name: String,
    adapter_ids: Vec<String>,
}

impl Db {
    /// List adapters for a tenant (for index building)
    pub async fn list_adapters(&self, tenant_id: &str) -> Result<Vec<AdapterInfo>> {
        let rows = sqlx::query(
            "SELECT adapter_id FROM adapters WHERE tenant_id = ? ORDER BY adapter_id"
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query adapters: {}", e)))?;

        let adapters = rows.into_iter().map(|row| {
            AdapterInfo {
                id: row.get("adapter_id"),
            }
        }).collect();

        Ok(adapters)
    }

    /// List adapter stacks for a tenant
    pub async fn list_adapter_stacks(&self, tenant_id: &str) -> Result<Vec<StackDb>> {
        let rows = sqlx::query(
            "SELECT name, adapter_ids_json FROM adapter_stacks WHERE tenant_id = ? ORDER BY name"
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query adapter stacks: {}", e)))?;

        let stacks = rows.into_iter().map(|row| {
            let name: String = row.get("name");
            let adapter_ids_json: String = row.get("adapter_ids_json");
            let adapter_ids: Vec<String> = serde_json::from_str(&adapter_ids_json).unwrap_or_default();
            StackDb { name, adapter_ids }
        }).collect();

        Ok(stacks)
    }
}
