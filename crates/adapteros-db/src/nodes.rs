use crate::Db;
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::{AosError, Result};
use adapteros_types::nodes::{Node, NodeDetail as NodeDetailType};
use serde::{Deserialize, Serialize};
use crate::new_id;
use adapteros_id::IdPrefix;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct NodeRow {
    pub id: String,
    pub hostname: String,
    pub agent_endpoint: String,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub labels_json: Option<String>,
    pub created_at: String,
}

impl From<NodeRow> for Node {
    fn from(row: NodeRow) -> Self {
        Self {
            id: row.id,
            hostname: row.hostname,
            agent_endpoint: row.agent_endpoint,
            status: row.status,
            last_seen_at: row.last_seen_at,
            labels_json: row.labels_json,
            created_at: row.created_at,
        }
    }
}

impl Db {
    pub async fn register_node(&self, hostname: &str, agent_endpoint: &str) -> Result<String> {
        let id = new_id(IdPrefix::Nod);
        sqlx::query(
            "INSERT INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
        )
        .bind(&id)
        .bind(hostname)
        .bind(agent_endpoint)
        .execute(self.pool())
        .await
        .db_err("register node")?;
        Ok(id)
    }

    pub async fn update_node_heartbeat(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE nodes SET last_seen_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .db_err("update node heartbeat")?;
        Ok(())
    }

    pub async fn list_nodes(&self) -> Result<Vec<Node>> {
        let nodes = sqlx::query_as::<_, NodeRow>(
            "SELECT id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at FROM nodes ORDER BY created_at DESC"
        )
        .fetch_all(self.pool())
        .await
        .db_err("list nodes")?;
        Ok(nodes.into_iter().map(Node::from).collect())
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let node = sqlx::query_as::<_, NodeRow>(
            "SELECT id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at FROM nodes WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .db_err("get node")?;
        Ok(node.map(Node::from))
    }

    pub async fn update_node_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE nodes SET status = ?, last_seen_at = datetime('now') WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(self.pool())
            .await
            .db_err("update node status")?;
        Ok(())
    }

    /// Delete a node
    pub async fn delete_node(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM nodes WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete node: {}", e)))?;
        Ok(())
    }

    /// Get list of adapter IDs loaded on a specific node
    pub async fn get_node_loaded_adapters(&self, node_id: &str) -> Result<Vec<String>> {
        let adapter_ids = sqlx::query_scalar::<_, String>(
            "SELECT adapter_id FROM adapters WHERE node_id = ? AND (current_state IN ('warm', 'hot', 'resident') OR load_state IN ('loaded', 'warm'))",
        )
        .bind(node_id)
        .fetch_all(self.pool())
        .await
        .db_err("get node loaded adapters")?;

        Ok(adapter_ids)
    }

    /// Check if a node is designated as primary
    ///
    /// Note: This assumes a 'is_primary' or similar column exists in the nodes table.
    /// Returns false if the column doesn't exist.
    pub async fn is_node_primary(&self, node_id: &str) -> Result<bool> {
        let is_primary =
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(is_primary, 0) FROM nodes WHERE id = ?")
                .bind(node_id)
                .fetch_optional(self.pool())
                .await
                .db_err("check if node is primary")?
                .unwrap_or(0);

        Ok(is_primary > 0)
    }

    /// Get detailed node information by ID
    pub async fn get_node_detail(&self, node_id: &str) -> Result<Option<NodeDetailType>> {
        let node = sqlx::query_as::<_, NodeRow>(
            "SELECT id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at
             FROM nodes WHERE id = ?",
        )
        .bind(node_id)
        .fetch_optional(self.pool())
        .await
        .db_err("get node detail")?;

        Ok(node.map(|r| NodeDetailType {
            node: Node::from(r),
            workers: Vec::new(), // Workers list should be populated separately or via join
        }))
    }

    /// Get adapters loaded on a node from workers
    ///
    /// This query retrieves unique adapter IDs from workers serving on a specific node.
    pub async fn get_node_adapters_from_workers(&self, node_id: &str) -> Result<Vec<String>> {
        let adapters = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT a.adapter_id
             FROM workers w
             JOIN adapters a ON a.id IN (SELECT json_extract(value, '$') FROM json_each(w.adapters_loaded_json))
             WHERE w.node_id = ? AND w.status = 'healthy'",
        )
        .bind(node_id)
        .fetch_all(self.pool())
        .await
        .db_err("get node adapters from workers")?;

        Ok(adapters)
    }

    /// Check if node is primary in federation
    pub async fn is_federation_primary(&self, node_id: &str) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM federation_config WHERE primary_node_id = ?",
        )
        .bind(node_id)
        .fetch_one(self.pool())
        .await
        .unwrap_or(0);

        Ok(count > 0)
    }
}
