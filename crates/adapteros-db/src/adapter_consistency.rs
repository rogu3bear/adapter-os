use crate::adapters::Adapter;
use crate::adapters_kv::AdapterKvOps;
use crate::Db;
use adapteros_core::{AosError, Result};

/// Consistency status between SQL and KV for an adapter.
#[derive(Debug, Clone)]
pub struct AdapterConsistency {
    pub adapter_id: String,
    pub tenant_id: String,
    pub kv_present: bool,
    pub hash_match: bool,
    pub aos_file_hash_match: bool,
    pub message: Option<String>,
}

impl AdapterConsistency {
    /// Ready for activation when KV is present and critical hashes align.
    pub fn is_ready(&self) -> bool {
        self.kv_present && self.hash_match && self.aos_file_hash_match
    }
}

impl Db {
    /// Compare SQL vs KV adapter records for presence and BLAKE3/hash alignment.
    ///
    /// Fails closed if KV backend is unavailable or KV record is missing.
    pub async fn check_adapter_consistency(&self, adapter_id: &str) -> Result<AdapterConsistency> {
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL pool unavailable for adapter consistency check".to_string())
        })?;

        #[derive(sqlx::FromRow)]
        struct AdapterHashes {
            #[allow(dead_code)]
            adapter_id: Option<String>,
            tenant_id: String,
            hash_b3: String,
            aos_file_hash: Option<String>,
        }

        let row = sqlx::query_as::<_, AdapterHashes>(
            r#"
            SELECT adapter_id, tenant_id, hash_b3, aos_file_hash
            FROM adapters
            WHERE adapter_id = ?
            "#,
        )
        .bind(adapter_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to fetch adapter for consistency: {}", e))
        })?;

        let row = row.ok_or_else(|| {
            AosError::NotFound(format!(
                "Adapter {} not found for SQL/KV consistency check",
                adapter_id
            ))
        })?;

        let tenant_id = row.tenant_id.clone();
        let kv_repo = self.get_adapter_kv_repo(&tenant_id).ok_or_else(|| {
            AosError::Validation(format!(
                "KV backend unavailable for adapter {} (tenant {})",
                adapter_id, tenant_id
            ))
        })?;

        let kv_adapter: Option<Adapter> = kv_repo.get_adapter_kv(adapter_id).await?;
        let mut status = AdapterConsistency {
            adapter_id: adapter_id.to_string(),
            tenant_id,
            kv_present: kv_adapter.is_some(),
            hash_match: false,
            aos_file_hash_match: false,
            message: None,
        };

        let Some(kv) = kv_adapter else {
            status.message = Some("KV adapter missing".to_string());
            return Ok(status);
        };

        status.hash_match = kv.hash_b3 == row.hash_b3;
        status.aos_file_hash_match = match (&row.aos_file_hash, &kv.aos_file_hash) {
            (Some(sql_hash), Some(kv_hash)) => sql_hash == kv_hash,
            (None, None) => true,
            // If SQL tracks a hash but KV does not (or vice versa), treat as mismatch.
            _ => false,
        };

        if !status.hash_match {
            status.message = Some("hash_b3 mismatch between SQL and KV".to_string());
        } else if !status.aos_file_hash_match {
            status.message = Some("aos_file_hash mismatch between SQL and KV".to_string());
        }

        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::AdapterConsistency;

    #[test]
    fn readiness_requires_all_flags() {
        let base = AdapterConsistency {
            adapter_id: "a1".into(),
            tenant_id: "t1".into(),
            kv_present: true,
            hash_match: true,
            aos_file_hash_match: true,
            message: None,
        };

        assert!(base.is_ready());
        assert!(!AdapterConsistency {
            kv_present: false,
            ..base.clone()
        }
        .is_ready());
        assert!(!AdapterConsistency {
            hash_match: false,
            ..base.clone()
        }
        .is_ready());
        assert!(!AdapterConsistency {
            aos_file_hash_match: false,
            ..base
        }
        .is_ready());
    }
}
