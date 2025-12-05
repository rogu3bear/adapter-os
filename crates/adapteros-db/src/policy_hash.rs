//! Policy hash storage for runtime integrity validation
//!
//! This module provides CRUD operations for policy pack baseline hashes.
//! Hashes are used to detect runtime policy mutations and trigger quarantine
//! per Determinism Ruleset #2.

use crate::Db;
use adapteros_core::{AosError, B3Hash};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::time::{SystemTime, UNIX_EPOCH};

/// Policy hash record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyHashRecord {
    pub policy_pack_id: String,
    pub baseline_hash: B3Hash,
    pub cpid: Option<String>,
    pub signer_pubkey: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Db {
    /// Insert a new policy hash baseline
    pub async fn insert_policy_hash(
        &self,
        policy_pack_id: &str,
        baseline_hash: &B3Hash,
        cpid: Option<&str>,
        signer_pubkey: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AosError::Database(format!("System time error: {}", e)))?
            .as_secs() as i64;

        let cpid_str = cpid.unwrap_or("global");
        let hash_hex = baseline_hash.to_hex();

        sqlx::query(
            "INSERT INTO policy_hashes (policy_pack_id, baseline_hash, cpid, signer_pubkey, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT(policy_pack_id, cpid) DO UPDATE SET
                baseline_hash = excluded.baseline_hash,
                signer_pubkey = excluded.signer_pubkey,
                updated_at = excluded.updated_at"
        )
        .bind(policy_pack_id)
        .bind(&hash_hex)
        .bind(cpid_str)
        .bind(signer_pubkey)
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;

        Ok(())
    }

    /// Get a policy hash record by policy pack ID and CPID
    pub async fn get_policy_hash(
        &self,
        policy_pack_id: &str,
        cpid: Option<&str>,
    ) -> anyhow::Result<Option<PolicyHashRecord>> {
        let cpid_str = cpid.unwrap_or("global");

        let row = sqlx::query(
            "SELECT policy_pack_id, baseline_hash, cpid, signer_pubkey, created_at, updated_at
             FROM policy_hashes
             WHERE policy_pack_id = $1 AND cpid = $2",
        )
        .bind(policy_pack_id)
        .bind(cpid_str)
        .fetch_optional(self.pool())
        .await?;

        if let Some(row) = row {
            let hash_hex: String = row.try_get("baseline_hash")?;
            let baseline_hash = B3Hash::from_hex(&hash_hex)
                .map_err(|e| AosError::Database(format!("Invalid hash in database: {}", e)))?;

            let cpid_value: String = row.try_get("cpid")?;
            let cpid = if cpid_value == "global" {
                None
            } else {
                Some(cpid_value)
            };

            Ok(Some(PolicyHashRecord {
                policy_pack_id: row.try_get("policy_pack_id")?,
                baseline_hash,
                cpid,
                signer_pubkey: row.try_get("signer_pubkey")?,
                created_at: row.try_get::<i64, _>("created_at")? as u64,
                updated_at: row.try_get::<i64, _>("updated_at")? as u64,
            }))
        } else {
            Ok(None)
        }
    }

    /// Update an existing policy hash
    pub async fn update_policy_hash(
        &self,
        policy_pack_id: &str,
        baseline_hash: &B3Hash,
        cpid: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AosError::Database(format!("System time error: {}", e)))?
            .as_secs() as i64;

        let cpid_str = cpid.unwrap_or("global");
        let hash_hex = baseline_hash.to_hex();

        let result = sqlx::query(
            "UPDATE policy_hashes
             SET baseline_hash = $1, updated_at = $2
             WHERE policy_pack_id = $3 AND cpid = $4",
        )
        .bind(&hash_hex)
        .bind(now)
        .bind(policy_pack_id)
        .bind(cpid_str)
        .execute(self.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Err(AosError::Database(format!(
                "Policy hash not found: {} (cpid: {})",
                policy_pack_id, cpid_str
            ))
            .into());
        }

        Ok(())
    }

    /// List all policy hashes for a given CPID (or all if None)
    pub async fn list_policy_hashes(
        &self,
        cpid: Option<&str>,
    ) -> anyhow::Result<Vec<PolicyHashRecord>> {
        let rows = if let Some(cpid_val) = cpid {
            let cpid_str = if cpid_val.is_empty() {
                "global"
            } else {
                cpid_val
            };
            sqlx::query(
                "SELECT policy_pack_id, baseline_hash, cpid, signer_pubkey, created_at, updated_at
                 FROM policy_hashes
                 WHERE cpid = $1
                 ORDER BY policy_pack_id",
            )
            .bind(cpid_str)
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query(
                "SELECT policy_pack_id, baseline_hash, cpid, signer_pubkey, created_at, updated_at
                 FROM policy_hashes
                 ORDER BY cpid, policy_pack_id",
            )
            .fetch_all(self.pool())
            .await?
        };

        let mut result = Vec::new();
        for row in rows {
            let hash_hex: String = row.try_get("baseline_hash")?;
            let baseline_hash = B3Hash::from_hex(&hash_hex)
                .map_err(|e| AosError::Database(format!("Invalid hash in database: {}", e)))?;

            let cpid_value: String = row.try_get("cpid")?;
            let cpid = if cpid_value == "global" {
                None
            } else {
                Some(cpid_value)
            };

            result.push(PolicyHashRecord {
                policy_pack_id: row.try_get("policy_pack_id")?,
                baseline_hash,
                cpid,
                signer_pubkey: row.try_get("signer_pubkey")?,
                created_at: row.try_get::<i64, _>("created_at")? as u64,
                updated_at: row.try_get::<i64, _>("updated_at")? as u64,
            });
        }

        Ok(result)
    }

    /// Delete a policy hash
    pub async fn delete_policy_hash(
        &self,
        policy_pack_id: &str,
        cpid: Option<&str>,
    ) -> anyhow::Result<()> {
        let cpid_str = cpid.unwrap_or("global");

        let result = sqlx::query(
            "DELETE FROM policy_hashes
             WHERE policy_pack_id = $1 AND cpid = $2",
        )
        .bind(policy_pack_id)
        .bind(cpid_str)
        .execute(self.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Err(AosError::Database(format!(
                "Policy hash not found: {} (cpid: {})",
                policy_pack_id, cpid_str
            ))
            .into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> Db {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_url = format!("sqlite://{}", db_path.display());

        let db = Db::connect(&db_url).await.unwrap();

        // Run migrations
        db.migrate().await.unwrap();

        db
    }

    #[tokio::test]
    async fn test_insert_and_get_policy_hash() {
        let db = setup_test_db().await;

        let hash = B3Hash::hash(b"test policy config");
        db.insert_policy_hash("test_policy", &hash, Some("cp-001"), Some("pubkey123"))
            .await
            .unwrap();

        let record = db
            .get_policy_hash("test_policy", Some("cp-001"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.policy_pack_id, "test_policy");
        assert_eq!(record.baseline_hash, hash);
        assert_eq!(record.cpid, Some("cp-001".to_string()));
        assert_eq!(record.signer_pubkey, Some("pubkey123".to_string()));
    }

    #[tokio::test]
    async fn test_update_policy_hash() {
        let db = setup_test_db().await;

        let hash1 = B3Hash::hash(b"config v1");
        db.insert_policy_hash("test_policy", &hash1, Some("cp-001"), None)
            .await
            .unwrap();

        let hash2 = B3Hash::hash(b"config v2");
        db.update_policy_hash("test_policy", &hash2, Some("cp-001"))
            .await
            .unwrap();

        let record = db
            .get_policy_hash("test_policy", Some("cp-001"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.baseline_hash, hash2);
    }

    #[tokio::test]
    async fn test_list_policy_hashes() {
        let db = setup_test_db().await;

        let hash1 = B3Hash::hash(b"config 1");
        let hash2 = B3Hash::hash(b"config 2");

        db.insert_policy_hash("policy1", &hash1, Some("cp-001"), None)
            .await
            .unwrap();
        db.insert_policy_hash("policy2", &hash2, Some("cp-001"), None)
            .await
            .unwrap();

        let records = db.list_policy_hashes(Some("cp-001")).await.unwrap();
        assert_eq!(records.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_policy_hash() {
        let db = setup_test_db().await;

        let hash = B3Hash::hash(b"test config");
        db.insert_policy_hash("test_policy", &hash, Some("cp-001"), None)
            .await
            .unwrap();

        db.delete_policy_hash("test_policy", Some("cp-001"))
            .await
            .unwrap();

        let record = db
            .get_policy_hash("test_policy", Some("cp-001"))
            .await
            .unwrap();
        assert!(record.is_none());
    }

    #[tokio::test]
    async fn test_global_cpid() {
        let db = setup_test_db().await;

        let hash = B3Hash::hash(b"global config");
        db.insert_policy_hash("global_policy", &hash, None, None)
            .await
            .unwrap();

        let record = db
            .get_policy_hash("global_policy", None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.cpid, None);
    }
}
