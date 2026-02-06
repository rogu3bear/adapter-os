use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;

/// Definition of a cluster in the semantic topology graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterDefinition {
    pub id: String,
    pub description: String,
    pub default_adapter_id: Option<String>,
    pub version: String,
    /// Human-readable display name derived from the cluster's typed ID word alias.
    /// Populated when the ID uses the TypedId format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Per-adapter topology metadata captured from the catalog.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterTopology {
    pub adapter_id: String,
    pub name: String,
    pub cluster_ids: Vec<String>,
    pub transition_probabilities: HashMap<String, f64>,
}

/// Adjacency entry from one cluster to a probable next cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdjacencyEdge {
    pub to_cluster_id: String,
    pub probability: f64,
}

/// Full topology graph snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TopologyGraph {
    pub clusters_version: String,
    pub clusters: Vec<ClusterDefinition>,
    pub adapters: Vec<AdapterTopology>,
    pub adjacency: HashMap<String, Vec<AdjacencyEdge>>,
}

impl Db {
    /// Create topology tables if they are missing. This is a runtime safety net to avoid
    /// schema drift when the unsigned crate migration is not applied in dev/test setups.
    pub async fn ensure_topology_schema(&self) -> Result<()> {
        if let Some(pool) = self.pool_opt() {
            // clusters table
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS clusters (
                     id TEXT PRIMARY KEY,
                     description TEXT NOT NULL,
                     default_adapter_id TEXT,
                     version TEXT NOT NULL,
                     created_at TEXT DEFAULT (datetime('now')),
                     updated_at TEXT DEFAULT (datetime('now'))
                 )",
            )
            .execute(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create clusters table: {}", e)))?;

            // topology-adapters table (lightweight metadata for UI)
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS topology_adapters (
                     adapter_id TEXT PRIMARY KEY,
                     name TEXT NOT NULL,
                     version TEXT NOT NULL
                 )",
            )
            .execute(pool)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to create topology_adapters table: {}", e))
            })?;

            // adapter_clusters join table
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS adapter_clusters (
                     adapter_id TEXT NOT NULL,
                     cluster_id TEXT NOT NULL,
                     PRIMARY KEY (adapter_id, cluster_id),
                     FOREIGN KEY(cluster_id) REFERENCES clusters(id)
                 )",
            )
            .execute(pool)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to create adapter_clusters table: {}", e))
            })?;

            // per-adapter transition probabilities (cluster -> next cluster)
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS adapter_cluster_transitions (
                     adapter_id TEXT NOT NULL,
                     to_cluster_id TEXT NOT NULL,
                     probability REAL NOT NULL,
                     PRIMARY KEY (adapter_id, to_cluster_id),
                     FOREIGN KEY(to_cluster_id) REFERENCES clusters(id)
                 )",
            )
            .execute(pool)
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to create adapter_cluster_transitions table: {}",
                    e
                ))
            })?;

            // Deterministic ordering helper indexes
            sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_adapter_clusters_cluster_id
                 ON adapter_clusters(cluster_id, adapter_id)",
            )
            .execute(pool)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to create adapter_clusters index: {}", e))
            })?;

            sqlx::query(
                "CREATE INDEX IF NOT EXISTS idx_adapter_cluster_transitions_cluster
                 ON adapter_cluster_transitions(to_cluster_id, adapter_id)",
            )
            .execute(pool)
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to create adapter_cluster_transitions index: {}",
                    e
                ))
            })?;
        }

        Ok(())
    }

    /// Replace topology state with the provided cluster and adapter metadata.
    /// All operations are performed transactionally to keep the graph consistent.
    pub async fn replace_topology(
        &self,
        clusters_version: &str,
        clusters: &[ClusterDefinition],
        adapters: &[AdapterTopology],
    ) -> Result<()> {
        self.ensure_topology_schema().await?;

        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for topology ingestion".to_string())
        })?;

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Clear existing graph
        sqlx::query("DELETE FROM adapter_cluster_transitions")
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        sqlx::query("DELETE FROM adapter_clusters")
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        sqlx::query("DELETE FROM topology_adapters")
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        sqlx::query("DELETE FROM clusters")
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Insert clusters deterministically (sorted by id)
        let mut sorted_clusters = clusters.to_vec();
        sorted_clusters.sort_by(|a, b| a.id.cmp(&b.id));
        for cluster in sorted_clusters {
            sqlx::query(
                "INSERT INTO clusters (id, description, default_adapter_id, version)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(&cluster.id)
            .bind(&cluster.description)
            .bind(&cluster.default_adapter_id)
            .bind(&cluster.version)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        }

        // Insert adapters and their cluster memberships / transitions
        let mut sorted_adapters = adapters.to_vec();
        sorted_adapters.sort_by(|a, b| a.adapter_id.cmp(&b.adapter_id));

        for adapter in sorted_adapters {
            sqlx::query(
                "INSERT INTO topology_adapters (adapter_id, name, version)
                 VALUES (?, ?, ?)",
            )
            .bind(&adapter.adapter_id)
            .bind(&adapter.name)
            .bind(clusters_version)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            // Ensure at least one cluster mapping
            if adapter.cluster_ids.is_empty() {
                return Err(AosError::Validation(format!(
                    "Adapter {} must belong to at least one cluster",
                    adapter.adapter_id
                )));
            }

            let mut cluster_ids = adapter.cluster_ids.clone();
            cluster_ids.sort();
            cluster_ids.dedup();

            for cluster_id in cluster_ids {
                sqlx::query("INSERT INTO adapter_clusters (adapter_id, cluster_id) VALUES (?, ?)")
                    .bind(&adapter.adapter_id)
                    .bind(&cluster_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AosError::Database(e.to_string()))?;
            }

            // Transition probabilities
            let mut transitions: Vec<(&String, &f64)> =
                adapter.transition_probabilities.iter().collect();
            transitions.sort_by(|a, b| a.0.cmp(b.0));
            for (to_cluster_id, prob) in transitions {
                sqlx::query(
                    "INSERT INTO adapter_cluster_transitions (adapter_id, to_cluster_id, probability)
                     VALUES (?, ?, ?)",
                )
                .bind(&adapter.adapter_id)
                .bind(to_cluster_id)
                .bind(prob)
                .execute(&mut *tx)
                .await
                .map_err(|e| AosError::Database(e.to_string()))?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(())
    }

    /// Returns an adjacency matrix mapping cluster -> ordered list of next-likely clusters.
    /// Ordering: probability DESC, to_cluster_id ASC to maintain determinism.
    pub async fn get_adjacency_matrix(&self) -> Result<HashMap<String, Vec<AdjacencyEdge>>> {
        self.ensure_topology_schema().await?;
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for topology adjacency".to_string())
        })?;

        let rows = sqlx::query(
            r#"
            SELECT ac.cluster_id as from_cluster,
                   act.to_cluster_id as to_cluster,
                   AVG(act.probability) as probability
            FROM adapter_clusters ac
            JOIN adapter_cluster_transitions act ON act.adapter_id = ac.adapter_id
            GROUP BY ac.cluster_id, act.to_cluster_id
            ORDER BY ac.cluster_id ASC, probability DESC, act.to_cluster_id ASC
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let mut adjacency: HashMap<String, Vec<AdjacencyEdge>> = HashMap::new();
        for row in rows {
            let from_cluster: String = row.get("from_cluster");
            let to_cluster: String = row.get("to_cluster");
            let probability: f64 = row.get::<f64, _>("probability");

            adjacency
                .entry(from_cluster)
                .or_default()
                .push(AdjacencyEdge {
                    to_cluster_id: to_cluster,
                    probability,
                });
        }

        Ok(adjacency)
    }

    /// Fetch the full topology graph, including clusters, adapters, and adjacency matrix.
    pub async fn get_topology_graph(&self) -> Result<TopologyGraph> {
        self.ensure_topology_schema().await?;
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for topology graph".to_string())
        })?;

        let clusters_rows = sqlx::query(
            "SELECT id, description, default_adapter_id, version FROM clusters ORDER BY id ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let clusters: Vec<ClusterDefinition> = clusters_rows
            .into_iter()
            .map(|row| ClusterDefinition {
                id: row.get("id"),
                description: row.get("description"),
                default_adapter_id: row.get("default_adapter_id"),
                version: row.get("version"),
                display_name: None,
            })
            .collect();

        let adapters_rows = sqlx::query(
            "SELECT adapter_id, name, version FROM topology_adapters ORDER BY adapter_id ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let mut adapters: Vec<AdapterTopology> = Vec::new();
        for row in adapters_rows {
            let adapter_id: String = row.get("adapter_id");
            let name: String = row.get("name");

            let cluster_rows = sqlx::query(
                "SELECT cluster_id FROM adapter_clusters WHERE adapter_id = ? ORDER BY cluster_id ASC",
            )
            .bind(&adapter_id)
            .fetch_all(pool)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
            let cluster_ids = cluster_rows
                .into_iter()
                .map(|r| r.get::<String, _>("cluster_id"))
                .collect::<Vec<_>>();

            let transition_rows = sqlx::query(
                "SELECT to_cluster_id, probability FROM adapter_cluster_transitions WHERE adapter_id = ? ORDER BY to_cluster_id ASC",
            )
            .bind(&adapter_id)
            .fetch_all(pool)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
            let transition_probabilities = transition_rows
                .into_iter()
                .map(|r| (r.get("to_cluster_id"), r.get::<f64, _>("probability")))
                .collect::<HashMap<_, _>>();

            adapters.push(AdapterTopology {
                adapter_id,
                name,
                cluster_ids,
                transition_probabilities,
            });
        }

        let clusters_version = clusters
            .first()
            .map(|c| c.version.clone())
            .unwrap_or_else(|| "1.0".to_string());

        let adjacency = self.get_adjacency_matrix().await?;

        Ok(TopologyGraph {
            clusters_version,
            clusters,
            adapters,
            adjacency,
        })
    }
}
