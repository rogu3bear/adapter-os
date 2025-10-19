use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Node {
    pub id: String,
    pub hostname: String,
    pub agent_endpoint: String,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub labels_json: Option<String>,
    pub created_at: String,
}

impl Db {
    pub async fn register_node(&self, hostname: &str, agent_endpoint: &str) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
        )
        .bind(&id)
        .bind(hostname)
        .bind(agent_endpoint)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn update_node_heartbeat(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE nodes SET last_seen_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn list_nodes(&self) -> Result<Vec<Node>> {
        let nodes = sqlx::query_as::<_, Node>(
            "SELECT id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at FROM nodes ORDER BY created_at DESC"
        )
        .fetch_all(self.pool())
        .await?;
        Ok(nodes)
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let node = sqlx::query_as::<_, Node>(
            "SELECT id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at FROM nodes WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(node)
    }

    pub async fn update_node_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE nodes SET status = ?, last_seen_at = datetime('now') WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
