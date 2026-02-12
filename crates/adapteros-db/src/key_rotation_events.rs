//! Key rotation event persistence
//!
//! Persists key rotation history to the database so audit trails survive
//! process restarts. Previously rotation events lived only in the
//! `RotationDaemon`'s in-memory `Vec<RotationHistoryEntry>`.
//!
//! ## Schema
//! See migration `20260211130000_key_rotation_events.sql`.

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// A persisted key rotation event.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KeyRotationEvent {
    pub id: String,
    pub key_fingerprint: String,
    pub rotation_type: String,
    pub rotated_at: String,
    pub rotated_by: String,
    pub prev_key_fingerprint: Option<String>,
    pub deks_reencrypted: i64,
    pub metadata: Option<String>,
}

impl Db {
    /// Record a key rotation event.
    ///
    /// The caller is responsible for generating a unique `id` (use
    /// `adapteros_id::TypedId::new(IdPrefix::Rot)`).
    pub async fn record_key_rotation(
        &self,
        id: &str,
        key_fingerprint: &str,
        rotation_type: &str,
        rotated_at: &str,
        rotated_by: &str,
        prev_key_fingerprint: Option<&str>,
        deks_reencrypted: i64,
        metadata: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO key_rotation_events (
                id, key_fingerprint, rotation_type, rotated_at,
                rotated_by, prev_key_fingerprint, deks_reencrypted, metadata
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(key_fingerprint)
        .bind(rotation_type)
        .bind(rotated_at)
        .bind(rotated_by)
        .bind(prev_key_fingerprint)
        .bind(deks_reencrypted)
        .bind(metadata)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to record key rotation event: {}", e)))?;

        Ok(())
    }

    /// List key rotation events ordered by `rotated_at` descending.
    ///
    /// Supports pagination via `limit` and `offset`.
    pub async fn list_key_rotations(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<KeyRotationEvent>> {
        let rows = sqlx::query_as::<_, KeyRotationEvent>(
            r#"
            SELECT id, key_fingerprint, rotation_type, rotated_at,
                   rotated_by, prev_key_fingerprint, deks_reencrypted, metadata
            FROM key_rotation_events
            ORDER BY rotated_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list key rotation events: {}", e)))?;

        Ok(rows)
    }

    /// Prune key rotation events older than `older_than_days` days.
    ///
    /// Default pruning threshold is 90 days. Returns the number of rows deleted.
    pub async fn prune_old_rotations(&self, older_than_days: i64) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM key_rotation_events
            WHERE rotated_at < datetime('now', ? || ' days')
            "#,
        )
        .bind(-older_than_days)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to prune old key rotation events: {}", e))
        })?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_and_list_key_rotation() {
        let db = Db::new_in_memory().await.unwrap();

        db.record_key_rotation(
            "rot-test-001",
            "fp-abc123",
            "scheduled",
            "2026-02-11T13:00:00Z",
            "rotation-daemon",
            None,
            0,
            Some(r#"{"note":"initial"}"#),
        )
        .await
        .unwrap();

        db.record_key_rotation(
            "rot-test-002",
            "fp-def456",
            "manual",
            "2026-02-11T14:00:00Z",
            "admin@example.com",
            Some("fp-abc123"),
            3,
            None,
        )
        .await
        .unwrap();

        let events = db.list_key_rotations(10, 0).await.unwrap();
        assert_eq!(events.len(), 2);
        // Most recent first
        assert_eq!(events[0].id, "rot-test-002");
        assert_eq!(events[0].rotation_type, "manual");
        assert_eq!(events[0].prev_key_fingerprint.as_deref(), Some("fp-abc123"));
        assert_eq!(events[0].deks_reencrypted, 3);
        assert_eq!(events[1].id, "rot-test-001");
        assert_eq!(events[1].rotation_type, "scheduled");
        assert!(events[1].prev_key_fingerprint.is_none());
    }

    #[tokio::test]
    async fn test_list_key_rotations_pagination() {
        let db = Db::new_in_memory().await.unwrap();

        for i in 0..5 {
            db.record_key_rotation(
                &format!("rot-page-{:03}", i),
                "fp-page",
                "scheduled",
                &format!("2026-02-11T{:02}:00:00Z", 10 + i),
                "daemon",
                None,
                0,
                None,
            )
            .await
            .unwrap();
        }

        let page1 = db.list_key_rotations(2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].id, "rot-page-004"); // newest first

        let page2 = db.list_key_rotations(2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = db.list_key_rotations(2, 4).await.unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn test_prune_old_rotations() {
        let db = Db::new_in_memory().await.unwrap();

        // Insert an event dated 100 days ago
        db.record_key_rotation(
            "rot-old-001",
            "fp-old",
            "scheduled",
            "2025-11-03T10:00:00Z",
            "daemon",
            None,
            0,
            None,
        )
        .await
        .unwrap();

        // Insert a recent event
        db.record_key_rotation(
            "rot-new-001",
            "fp-new",
            "manual",
            "2026-02-11T10:00:00Z",
            "admin",
            Some("fp-old"),
            1,
            None,
        )
        .await
        .unwrap();

        // Prune events older than 90 days
        let pruned = db.prune_old_rotations(90).await.unwrap();
        assert_eq!(pruned, 1);

        let remaining = db.list_key_rotations(10, 0).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "rot-new-001");
    }

    #[tokio::test]
    async fn test_rotation_type_constraint() {
        let db = Db::new_in_memory().await.unwrap();

        let result = db
            .record_key_rotation(
                "rot-bad-001",
                "fp-bad",
                "invalid_type",
                "2026-02-11T10:00:00Z",
                "daemon",
                None,
                0,
                None,
            )
            .await;

        assert!(result.is_err());
    }
}
