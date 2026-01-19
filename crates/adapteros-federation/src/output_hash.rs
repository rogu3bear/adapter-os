//! Output Hash Comparison - Cross-host determinism verification
//!
//! Compares inference output hashes across multiple hosts to verify
//! deterministic execution

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::Db;
use adapteros_telemetry::{LogLevel, TelemetryEventBuilder, TelemetryWriter};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Output hash record for cross-host comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputHashRecord {
    pub id: Option<String>,
    pub session_id: String,
    pub host_id: String,
    pub output_hash: B3Hash,
    pub input_hash: B3Hash,
    pub computed_at: String,
    pub deterministic: bool,
}

/// Cross-host comparison result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    pub session_id: String,
    pub input_hash: B3Hash,
    pub consistent: bool,
    pub hosts: Vec<String>,
    pub output_hashes: HashMap<String, B3Hash>,
    pub divergence_count: usize,
}

impl ComparisonResult {
    pub fn is_consistent(&self) -> bool {
        self.consistent
    }
}

/// Output hash manager for cross-host verification
pub struct OutputHashManager {
    db: Arc<Db>,
    telemetry: Option<Arc<TelemetryWriter>>,
}

impl OutputHashManager {
    /// Create a new output hash manager
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            telemetry: None,
        }
    }

    /// Create with telemetry writer
    pub fn with_telemetry(db: Arc<Db>, telemetry: Arc<TelemetryWriter>) -> Self {
        Self {
            db,
            telemetry: Some(telemetry),
        }
    }

    /// Record an output hash
    pub async fn record_output_hash(
        &self,
        session_id: String,
        host_id: String,
        output_hash: B3Hash,
        input_hash: B3Hash,
        deterministic: bool,
    ) -> Result<()> {
        debug!(
            session_id = %session_id,
            host_id = %host_id,
            output_hash = %output_hash.to_hex(),
            input_hash = %input_hash.to_hex(),
            deterministic = deterministic,
            "Recording output hash"
        );

        let pool = self.db.pool();
        let output_hash_hex = output_hash.to_hex();
        let input_hash_hex = input_hash.to_hex();
        let deterministic_val = if deterministic { 1 } else { 0 };

        sqlx::query(
            r#"
            INSERT INTO federation_output_hashes (session_id, host_id, output_hash, input_hash, computed_at, deterministic)
            VALUES (?, ?, ?, ?, datetime('now'), ?)
            "#
        )
        .bind(&session_id)
        .bind(&host_id)
        .bind(&output_hash_hex)
        .bind(&input_hash_hex)
        .bind(deterministic_val)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to record output hash: {}", e)))?;

        Ok(())
    }

    /// Get output hashes for a session
    pub async fn get_session_hashes(&self, session_id: &str) -> Result<Vec<OutputHashRecord>> {
        let pool = self.db.pool();

        let rows = sqlx::query(
            r#"
            SELECT id, session_id, host_id, output_hash, input_hash, computed_at, deterministic
            FROM federation_output_hashes
            WHERE session_id = ?
            ORDER BY computed_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch session hashes: {}", e)))?;

        let mut records = Vec::new();
        for row in rows {
            let output_hash_hex: String = row
                .try_get("output_hash")
                .map_err(|e| AosError::Database(format!("Failed to get output_hash: {}", e)))?;
            let input_hash_hex: String = row
                .try_get("input_hash")
                .map_err(|e| AosError::Database(format!("Failed to get input_hash: {}", e)))?;

            let output_hash = B3Hash::from_hex(&output_hash_hex)?;
            let input_hash = B3Hash::from_hex(&input_hash_hex)?;

            records.push(OutputHashRecord {
                id: Some(row.try_get("id").unwrap()),
                session_id: row.try_get("session_id").unwrap(),
                host_id: row.try_get("host_id").unwrap(),
                output_hash,
                input_hash,
                computed_at: row.try_get("computed_at").unwrap(),
                deterministic: row.try_get::<i64, _>("deterministic").unwrap() != 0,
            });
        }

        Ok(records)
    }

    /// Compare output hashes across hosts for a session
    pub async fn compare_session(&self, session_id: &str) -> Result<ComparisonResult> {
        let records = self.get_session_hashes(session_id).await?;

        if records.is_empty() {
            return Err(AosError::Validation(format!(
                "No output hashes found for session: {}",
                session_id
            )));
        }

        // Group by input hash
        let mut by_input: HashMap<String, Vec<OutputHashRecord>> = HashMap::new();
        for record in records {
            by_input
                .entry(record.input_hash.to_hex())
                .or_default()
                .push(record);
        }

        // For simplicity, take the first input hash
        let (input_hash_hex, records) = by_input.into_iter().next().unwrap();
        let input_hash = B3Hash::from_hex(&input_hash_hex)?;

        // Compare output hashes
        let mut output_hashes: HashMap<String, B3Hash> = HashMap::new();
        let mut hosts = Vec::new();

        for record in &records {
            output_hashes.insert(record.host_id.clone(), record.output_hash);
            hosts.push(record.host_id.clone());
        }

        // Check consistency
        let unique_hashes: std::collections::HashSet<_> = output_hashes.values().collect();
        let consistent = unique_hashes.len() == 1;
        let divergence_count = if consistent {
            0
        } else {
            unique_hashes.len() - 1
        };

        let result = ComparisonResult {
            session_id: session_id.to_string(),
            input_hash,
            consistent,
            hosts: hosts.clone(),
            output_hashes: output_hashes.clone(),
            divergence_count,
        };

        // Emit telemetry event (100% sampling)
        if let Some(ref telemetry) = self.telemetry {
            let event_type = if consistent {
                "federation.output_hash_match"
            } else {
                "federation.output_hash_mismatch"
            };

            let identity = adapteros_core::identity::IdentityEnvelope::new(
                "system".to_string(),
                "federation".to_string(),
                "verification".to_string(),
                "1.0".to_string(),
            );
            match TelemetryEventBuilder::new(
                adapteros_telemetry::EventType::Custom(event_type.to_string()),
                if consistent {
                    LogLevel::Info
                } else {
                    LogLevel::Warn
                },
                format!(
                    "Output hash comparison: {} (consistent: {})",
                    session_id, consistent
                ),
                identity,
            )
            .component("adapteros-federation".to_string())
            .metadata(json!({
                "session_id": session_id,
                "consistent": consistent,
                "hosts": hosts,
                "divergence_count": divergence_count,
            }))
            .build()
            {
                Ok(event) => {
                    let _ = telemetry.log_event(event);
                }
                Err(e) => {
                    warn!(error = %e, "Failed to build telemetry event for output hash comparison");
                }
            }
        }

        if consistent {
            info!(
                session_id = %session_id,
                hosts = ?hosts,
                "Output hashes are consistent across hosts"
            );
        } else {
            warn!(
                session_id = %session_id,
                hosts = ?hosts,
                divergence_count = divergence_count,
                "Output hash mismatch detected across hosts"
            );
        }

        Ok(result)
    }

    /// Get output hashes by input hash (for finding reproducible runs)
    pub async fn get_by_input_hash(&self, input_hash: &B3Hash) -> Result<Vec<OutputHashRecord>> {
        let pool = self.db.pool();
        let input_hash_hex = input_hash.to_hex();

        let rows = sqlx::query(
            r#"
            SELECT id, session_id, host_id, output_hash, input_hash, computed_at, deterministic
            FROM federation_output_hashes
            WHERE input_hash = ?
            ORDER BY computed_at ASC
            "#,
        )
        .bind(&input_hash_hex)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch by input hash: {}", e)))?;

        let mut records = Vec::new();
        for row in rows {
            let output_hash_hex: String = row
                .try_get("output_hash")
                .map_err(|e| AosError::Database(format!("Failed to get output_hash: {}", e)))?;
            let input_hash_hex: String = row
                .try_get("input_hash")
                .map_err(|e| AosError::Database(format!("Failed to get input_hash: {}", e)))?;

            let output_hash = B3Hash::from_hex(&output_hash_hex)?;
            let input_hash = B3Hash::from_hex(&input_hash_hex)?;

            records.push(OutputHashRecord {
                id: Some(row.try_get("id").unwrap()),
                session_id: row.try_get("session_id").unwrap(),
                host_id: row.try_get("host_id").unwrap(),
                output_hash,
                input_hash,
                computed_at: row.try_get("computed_at").unwrap(),
                deterministic: row.try_get::<i64, _>("deterministic").unwrap() != 0,
            });
        }

        Ok(records)
    }

    /// Check if all hosts produce the same output for a given input
    pub async fn verify_determinism(&self, input_hash: &B3Hash) -> Result<bool> {
        let records = self.get_by_input_hash(input_hash).await?;

        if records.is_empty() {
            return Ok(true); // No data, assume deterministic
        }

        // Check if all output hashes are the same
        let first_output = &records[0].output_hash;
        let all_same = records.iter().all(|r| &r.output_hash == first_output);

        Ok(all_same)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::PeerRegistry;
    use adapteros_crypto::Keypair;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    async fn setup_test_db() -> Result<Db> {
        let temp_dir = new_test_tempdir();
        let db_path = temp_dir.path().join("test.db");
        let db = Db::connect(db_path.to_str().unwrap()).await?;
        db.migrate().await?;
        std::mem::forget(temp_dir);
        Ok(db)
    }

    async fn register_peer(db: &Arc<Db>, host_id: &str) -> Result<()> {
        let registry = PeerRegistry::new(db.clone());
        registry
            .register_peer(
                host_id.to_string(),
                Keypair::generate().public_key(),
                None,
                None,
            )
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_record_and_get_output_hash() -> Result<()> {
        let db = Arc::new(setup_test_db().await?);
        register_peer(&db, "test-host").await?;
        let manager = OutputHashManager::new(db);

        let session_id = "test-session".to_string();
        let host_id = "test-host".to_string();
        let output_hash = B3Hash::hash(b"test output");
        let input_hash = B3Hash::hash(b"test input");

        manager
            .record_output_hash(
                session_id.clone(),
                host_id.clone(),
                output_hash,
                input_hash,
                true,
            )
            .await?;

        let records = manager.get_session_hashes(&session_id).await?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].host_id, host_id);
        assert_eq!(records[0].output_hash, output_hash);

        Ok(())
    }

    #[tokio::test]
    async fn test_compare_consistent_outputs() -> Result<()> {
        let db = Arc::new(setup_test_db().await?);
        register_peer(&db, "host1").await?;
        register_peer(&db, "host2").await?;
        let manager = OutputHashManager::new(db);

        let session_id = "test-session".to_string();
        let output_hash = B3Hash::hash(b"test output");
        let input_hash = B3Hash::hash(b"test input");

        // Record from two hosts with same output
        manager
            .record_output_hash(
                session_id.clone(),
                "host1".to_string(),
                output_hash,
                input_hash,
                true,
            )
            .await?;
        manager
            .record_output_hash(
                session_id.clone(),
                "host2".to_string(),
                output_hash,
                input_hash,
                true,
            )
            .await?;

        let result = manager.compare_session(&session_id).await?;
        assert!(result.is_consistent());
        assert_eq!(result.divergence_count, 0);
        assert_eq!(result.hosts.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_compare_divergent_outputs() -> Result<()> {
        let db = Arc::new(setup_test_db().await?);
        register_peer(&db, "host1").await?;
        register_peer(&db, "host2").await?;
        let manager = OutputHashManager::new(db);

        let session_id = "test-session".to_string();
        let input_hash = B3Hash::hash(b"test input");
        let output_hash1 = B3Hash::hash(b"output 1");
        let output_hash2 = B3Hash::hash(b"output 2");

        // Record from two hosts with different outputs
        manager
            .record_output_hash(
                session_id.clone(),
                "host1".to_string(),
                output_hash1,
                input_hash,
                true,
            )
            .await?;
        manager
            .record_output_hash(
                session_id.clone(),
                "host2".to_string(),
                output_hash2,
                input_hash,
                true,
            )
            .await?;

        let result = manager.compare_session(&session_id).await?;
        assert!(!result.is_consistent());
        assert_eq!(result.divergence_count, 1);

        Ok(())
    }
}
