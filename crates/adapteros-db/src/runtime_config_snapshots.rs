use crate::query_helpers::{db_err, serde_err};
use crate::Db;
use adapteros_core::Result;
use serde::{Deserialize, Serialize};

/// Persisted runtime config snapshot record.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RuntimeConfigSnapshotRecord {
    pub id: String,
    pub version: i64,
    pub schema_version: String,
    pub source: String,
    pub checksum_b3: String,
    pub settings_json: String,
    pub pending_restart_fields_json: Option<String>,
    pub updated_by: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewRuntimeConfigSnapshot {
    pub version: i64,
    pub schema_version: String,
    pub source: String,
    pub checksum_b3: String,
    pub settings_json: String,
    pub pending_restart_fields: Vec<String>,
    pub updated_by: Option<String>,
}

impl Db {
    async fn ensure_runtime_config_snapshots_table(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS runtime_config_snapshots (
                id TEXT PRIMARY KEY,
                version INTEGER NOT NULL,
                schema_version TEXT NOT NULL,
                source TEXT NOT NULL,
                checksum_b3 TEXT NOT NULL,
                settings_json TEXT NOT NULL,
                pending_restart_fields_json TEXT,
                updated_by TEXT,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(self.pool_result()?)
        .await
        .map_err(db_err("ensure runtime_config_snapshots table"))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_runtime_config_snapshots_version ON runtime_config_snapshots(version DESC)",
        )
        .execute(self.pool_result()?)
        .await
        .map_err(db_err("ensure runtime_config_snapshots version index"))?;

        Ok(())
    }

    pub async fn upsert_runtime_config_snapshot(
        &self,
        snapshot: &NewRuntimeConfigSnapshot,
    ) -> Result<RuntimeConfigSnapshotRecord> {
        self.ensure_runtime_config_snapshots_table().await?;

        let id = format!("rcfg-{}", snapshot.version);
        let pending_restart_fields_json =
            serde_json::to_string(&snapshot.pending_restart_fields).map_err(serde_err)?;

        sqlx::query(
            r#"
            INSERT INTO runtime_config_snapshots (
                id,
                version,
                schema_version,
                source,
                checksum_b3,
                settings_json,
                pending_restart_fields_json,
                updated_by,
                updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(id) DO UPDATE SET
                version = excluded.version,
                schema_version = excluded.schema_version,
                source = excluded.source,
                checksum_b3 = excluded.checksum_b3,
                settings_json = excluded.settings_json,
                pending_restart_fields_json = excluded.pending_restart_fields_json,
                updated_by = excluded.updated_by,
                updated_at = datetime('now')
            "#,
        )
        .bind(&id)
        .bind(snapshot.version)
        .bind(&snapshot.schema_version)
        .bind(&snapshot.source)
        .bind(&snapshot.checksum_b3)
        .bind(&snapshot.settings_json)
        .bind(&pending_restart_fields_json)
        .bind(&snapshot.updated_by)
        .execute(self.pool_result()?)
        .await
        .map_err(db_err("upsert runtime_config_snapshot"))?;

        let row = sqlx::query_as::<_, RuntimeConfigSnapshotRecord>(
            r#"
            SELECT id, version, schema_version, source, checksum_b3, settings_json,
                   pending_restart_fields_json, updated_by, updated_at
            FROM runtime_config_snapshots
            WHERE id = ?
            "#,
        )
        .bind(&id)
        .fetch_one(self.pool_result()?)
        .await
        .map_err(db_err("get runtime_config_snapshot after upsert"))?;

        Ok(row)
    }

    pub async fn get_latest_runtime_config_snapshot(
        &self,
    ) -> Result<Option<RuntimeConfigSnapshotRecord>> {
        self.ensure_runtime_config_snapshots_table().await?;

        let row = sqlx::query_as::<_, RuntimeConfigSnapshotRecord>(
            r#"
            SELECT id, version, schema_version, source, checksum_b3, settings_json,
                   pending_restart_fields_json, updated_by, updated_at
            FROM runtime_config_snapshots
            ORDER BY version DESC, updated_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(db_err("get latest runtime_config_snapshot"))?;

        Ok(row)
    }
}
