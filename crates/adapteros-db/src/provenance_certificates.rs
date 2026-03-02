//! Provenance certificate persistence
//!
//! Stores signed provenance certificates that capture the full lineage of an
//! adapter version — training data, checkpoint hashes, promotion history,
//! policy packs, and egress attestations.
//!
//! ## Schema
//! See migration `20260212000000_provenance_certificates.sql`.

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// A provenance certificate row from the database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProvenanceCertificateRecord {
    pub id: i64,
    pub certificate_id: String,
    pub adapter_id: String,
    pub version_id: String,
    pub tenant_id: String,

    // Training provenance
    pub training_data_hash: Option<String>,
    pub training_config_hash: Option<String>,
    pub training_job_id: Option<String>,
    pub training_final_loss: Option<f64>,
    pub training_epochs: Option<i64>,

    // Checkpoint provenance
    pub checkpoint_hash: Option<String>,
    pub checkpoint_signature: Option<String>,
    pub checkpoint_signer_key: Option<String>,

    // Promotion provenance
    pub promotion_review_id: Option<String>,
    pub promoted_by: Option<String>,
    pub promoted_at: Option<String>,
    pub promoted_from_state: Option<String>,
    pub promoted_to_state: Option<String>,

    // Serving provenance
    pub policy_pack_hash: Option<String>,
    pub policy_pack_id: Option<String>,
    pub base_model_id: Option<String>,

    // Egress attestation
    pub egress_blocked: Option<i64>,
    pub egress_rules_fingerprint: Option<String>,

    // Certificate metadata
    pub generated_at: String,
    pub content_hash: String,
    pub signature: String,
    pub signer_public_key: String,
    pub schema_version: i64,

    pub created_at: String,
}

/// Parameters for inserting a new provenance certificate.
///
/// Does not include `id` (auto-incremented) or `created_at` (defaulted by DB).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProvenanceCertificate {
    pub certificate_id: String,
    pub adapter_id: String,
    pub version_id: String,
    pub tenant_id: String,

    pub training_data_hash: Option<String>,
    pub training_config_hash: Option<String>,
    pub training_job_id: Option<String>,
    pub training_final_loss: Option<f64>,
    pub training_epochs: Option<i64>,

    pub checkpoint_hash: Option<String>,
    pub checkpoint_signature: Option<String>,
    pub checkpoint_signer_key: Option<String>,

    pub promotion_review_id: Option<String>,
    pub promoted_by: Option<String>,
    pub promoted_at: Option<String>,
    pub promoted_from_state: Option<String>,
    pub promoted_to_state: Option<String>,

    pub policy_pack_hash: Option<String>,
    pub policy_pack_id: Option<String>,
    pub base_model_id: Option<String>,

    pub egress_blocked: Option<i64>,
    pub egress_rules_fingerprint: Option<String>,

    pub generated_at: String,
    pub content_hash: String,
    pub signature: String,
    pub signer_public_key: String,
    pub schema_version: i64,
}

impl Db {
    /// Store a new provenance certificate. Returns the auto-generated row ID.
    pub async fn store_provenance_certificate(
        &self,
        record: &NewProvenanceCertificate,
    ) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO provenance_certificates (
                certificate_id, adapter_id, version_id, tenant_id,
                training_data_hash, training_config_hash, training_job_id,
                training_final_loss, training_epochs,
                checkpoint_hash, checkpoint_signature, checkpoint_signer_key,
                promotion_review_id, promoted_by, promoted_at,
                promoted_from_state, promoted_to_state,
                policy_pack_hash, policy_pack_id, base_model_id,
                egress_blocked, egress_rules_fingerprint,
                generated_at, content_hash, signature, signer_public_key,
                schema_version
            )
            VALUES (
                ?, ?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?,
                ?, ?,
                ?, ?, ?, ?,
                ?
            )
            "#,
        )
        .bind(&record.certificate_id)
        .bind(&record.adapter_id)
        .bind(&record.version_id)
        .bind(&record.tenant_id)
        .bind(&record.training_data_hash)
        .bind(&record.training_config_hash)
        .bind(&record.training_job_id)
        .bind(record.training_final_loss)
        .bind(record.training_epochs)
        .bind(&record.checkpoint_hash)
        .bind(&record.checkpoint_signature)
        .bind(&record.checkpoint_signer_key)
        .bind(&record.promotion_review_id)
        .bind(&record.promoted_by)
        .bind(&record.promoted_at)
        .bind(&record.promoted_from_state)
        .bind(&record.promoted_to_state)
        .bind(&record.policy_pack_hash)
        .bind(&record.policy_pack_id)
        .bind(&record.base_model_id)
        .bind(record.egress_blocked)
        .bind(&record.egress_rules_fingerprint)
        .bind(&record.generated_at)
        .bind(&record.content_hash)
        .bind(&record.signature)
        .bind(&record.signer_public_key)
        .bind(record.schema_version)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to store provenance certificate: {}", e))
        })?;

        Ok(result.last_insert_rowid())
    }

    /// Look up a provenance certificate by its certificate ID.
    pub async fn get_provenance_certificate(
        &self,
        certificate_id: &str,
    ) -> Result<Option<ProvenanceCertificateRecord>> {
        let row = sqlx::query_as::<_, ProvenanceCertificateRecord>(
            r#"
            SELECT *
            FROM provenance_certificates
            WHERE certificate_id = ?
            "#,
        )
        .bind(certificate_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get provenance certificate: {}", e)))?;

        Ok(row)
    }

    /// Get all provenance certificates for an adapter, newest first.
    pub async fn get_provenance_for_adapter(
        &self,
        adapter_id: &str,
    ) -> Result<Vec<ProvenanceCertificateRecord>> {
        let rows = sqlx::query_as::<_, ProvenanceCertificateRecord>(
            r#"
            SELECT *
            FROM provenance_certificates
            WHERE adapter_id = ?
            ORDER BY generated_at DESC
            "#,
        )
        .bind(adapter_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to get provenance certificates for adapter: {}",
                e
            ))
        })?;

        Ok(rows)
    }

    /// Get the latest provenance certificate for a specific version.
    pub async fn get_provenance_for_version(
        &self,
        version_id: &str,
    ) -> Result<Option<ProvenanceCertificateRecord>> {
        let row = sqlx::query_as::<_, ProvenanceCertificateRecord>(
            r#"
            SELECT *
            FROM provenance_certificates
            WHERE version_id = ?
            ORDER BY generated_at DESC
            LIMIT 1
            "#,
        )
        .bind(version_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to get provenance certificate for version: {}",
                e
            ))
        })?;

        Ok(row)
    }

    /// Get the latest provenance certificate for an adapter within a tenant.
    pub async fn get_latest_provenance(
        &self,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<Option<ProvenanceCertificateRecord>> {
        let row = sqlx::query_as::<_, ProvenanceCertificateRecord>(
            r#"
            SELECT *
            FROM provenance_certificates
            WHERE adapter_id = ? AND tenant_id = ?
            ORDER BY generated_at DESC
            LIMIT 1
            "#,
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to get latest provenance certificate: {}",
                e
            ))
        })?;

        Ok(row)
    }

    /// List provenance certificates for a tenant with pagination.
    pub async fn list_provenance_certificates(
        &self,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ProvenanceCertificateRecord>> {
        let rows = sqlx::query_as::<_, ProvenanceCertificateRecord>(
            r#"
            SELECT *
            FROM provenance_certificates
            WHERE tenant_id = ?
            ORDER BY generated_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to list provenance certificates: {}", e))
        })?;

        Ok(rows)
    }

    /// List provenance certificates for a specific adapter with pagination.
    pub async fn list_provenance_certificates_for_adapter(
        &self,
        adapter_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ProvenanceCertificateRecord>> {
        let rows = sqlx::query_as::<_, ProvenanceCertificateRecord>(
            r#"
            SELECT *
            FROM provenance_certificates
            WHERE adapter_id = ?
            ORDER BY generated_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(adapter_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to list provenance certificates for adapter: {}",
                e
            ))
        })?;

        Ok(rows)
    }

    /// Count provenance certificates for a specific adapter.
    pub async fn count_provenance_certificates_for_adapter(&self, adapter_id: &str) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM provenance_certificates
            WHERE adapter_id = ?
            "#,
        )
        .bind(adapter_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to count provenance certificates for adapter: {}",
                e
            ))
        })?;

        Ok(row.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cert(
        certificate_id: &str,
        adapter_id: &str,
        version_id: &str,
        tenant_id: &str,
        generated_at: &str,
    ) -> NewProvenanceCertificate {
        NewProvenanceCertificate {
            certificate_id: certificate_id.to_string(),
            adapter_id: adapter_id.to_string(),
            version_id: version_id.to_string(),
            tenant_id: tenant_id.to_string(),
            training_data_hash: Some("hash-data-001".to_string()),
            training_config_hash: Some("hash-config-001".to_string()),
            training_job_id: Some("job-001".to_string()),
            training_final_loss: Some(0.042),
            training_epochs: Some(5),
            checkpoint_hash: Some("ckpt-hash-001".to_string()),
            checkpoint_signature: Some("ckpt-sig-001".to_string()),
            checkpoint_signer_key: Some("ckpt-key-001".to_string()),
            promotion_review_id: Some("rvw-001".to_string()),
            promoted_by: Some("admin@example.com".to_string()),
            promoted_at: Some("2026-02-12T10:00:00Z".to_string()),
            promoted_from_state: Some("draft".to_string()),
            promoted_to_state: Some("active".to_string()),
            policy_pack_hash: Some("pp-hash-001".to_string()),
            policy_pack_id: Some("pp-001".to_string()),
            base_model_id: Some("mdl-llama-3b".to_string()),
            egress_blocked: Some(1),
            egress_rules_fingerprint: Some("egress-fp-001".to_string()),
            generated_at: generated_at.to_string(),
            content_hash: format!("content-hash-{}", certificate_id),
            signature: format!("sig-{}", certificate_id),
            signer_public_key: "pub-key-001".to_string(),
            schema_version: 1,
        }
    }

    #[tokio::test]
    async fn test_store_and_retrieve_round_trip() {
        let db = Db::new_in_memory().await.unwrap();
        let cert = make_cert(
            "cert-001",
            "adp-001",
            "ver-001",
            "tnt-001",
            "2026-02-12T12:00:00Z",
        );

        let row_id = db.store_provenance_certificate(&cert).await.unwrap();
        assert!(row_id > 0);

        let retrieved = db
            .get_provenance_certificate("cert-001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.certificate_id, "cert-001");
        assert_eq!(retrieved.adapter_id, "adp-001");
        assert_eq!(retrieved.version_id, "ver-001");
        assert_eq!(retrieved.tenant_id, "tnt-001");
        assert_eq!(
            retrieved.training_data_hash.as_deref(),
            Some("hash-data-001")
        );
        assert_eq!(retrieved.training_final_loss, Some(0.042));
        assert_eq!(retrieved.training_epochs, Some(5));
        assert_eq!(retrieved.checkpoint_hash.as_deref(), Some("ckpt-hash-001"));
        assert_eq!(retrieved.promoted_by.as_deref(), Some("admin@example.com"));
        assert_eq!(retrieved.promoted_from_state.as_deref(), Some("draft"));
        assert_eq!(retrieved.promoted_to_state.as_deref(), Some("active"));
        assert_eq!(retrieved.egress_blocked, Some(1));
        assert_eq!(retrieved.signature, "sig-cert-001");
        assert_eq!(retrieved.schema_version, 1);
    }

    #[tokio::test]
    async fn test_get_by_adapter_returns_multiple() {
        let db = Db::new_in_memory().await.unwrap();

        let c1 = make_cert(
            "cert-a1",
            "adp-100",
            "ver-100",
            "tnt-001",
            "2026-02-12T10:00:00Z",
        );
        let c2 = make_cert(
            "cert-a2",
            "adp-100",
            "ver-101",
            "tnt-001",
            "2026-02-12T11:00:00Z",
        );
        let c3 = make_cert(
            "cert-b1",
            "adp-200",
            "ver-200",
            "tnt-001",
            "2026-02-12T12:00:00Z",
        );

        db.store_provenance_certificate(&c1).await.unwrap();
        db.store_provenance_certificate(&c2).await.unwrap();
        db.store_provenance_certificate(&c3).await.unwrap();

        let certs = db.get_provenance_for_adapter("adp-100").await.unwrap();
        assert_eq!(certs.len(), 2);
        // newest first
        assert_eq!(certs[0].certificate_id, "cert-a2");
        assert_eq!(certs[1].certificate_id, "cert-a1");

        let certs_other = db.get_provenance_for_adapter("adp-200").await.unwrap();
        assert_eq!(certs_other.len(), 1);
        assert_eq!(certs_other[0].certificate_id, "cert-b1");
    }

    #[tokio::test]
    async fn test_get_by_version_returns_specific() {
        let db = Db::new_in_memory().await.unwrap();

        let c1 = make_cert(
            "cert-v1",
            "adp-001",
            "ver-specific",
            "tnt-001",
            "2026-02-12T10:00:00Z",
        );
        let c2 = make_cert(
            "cert-v2",
            "adp-001",
            "ver-other",
            "tnt-001",
            "2026-02-12T11:00:00Z",
        );

        db.store_provenance_certificate(&c1).await.unwrap();
        db.store_provenance_certificate(&c2).await.unwrap();

        let result = db
            .get_provenance_for_version("ver-specific")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.certificate_id, "cert-v1");

        let none = db
            .get_provenance_for_version("ver-nonexistent")
            .await
            .unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_get_latest_provenance() {
        let db = Db::new_in_memory().await.unwrap();

        let c1 = make_cert(
            "cert-l1",
            "adp-300",
            "ver-300",
            "tnt-010",
            "2026-02-12T08:00:00Z",
        );
        let c2 = make_cert(
            "cert-l2",
            "adp-300",
            "ver-301",
            "tnt-010",
            "2026-02-12T09:00:00Z",
        );
        let c3 = make_cert(
            "cert-l3",
            "adp-300",
            "ver-302",
            "tnt-020",
            "2026-02-12T10:00:00Z",
        );

        db.store_provenance_certificate(&c1).await.unwrap();
        db.store_provenance_certificate(&c2).await.unwrap();
        db.store_provenance_certificate(&c3).await.unwrap();

        let latest = db
            .get_latest_provenance("adp-300", "tnt-010")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.certificate_id, "cert-l2");

        let latest_other = db
            .get_latest_provenance("adp-300", "tnt-020")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest_other.certificate_id, "cert-l3");

        let none = db
            .get_latest_provenance("adp-300", "tnt-999")
            .await
            .unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_pagination() {
        let db = Db::new_in_memory().await.unwrap();

        for i in 0..5 {
            let cert = make_cert(
                &format!("cert-pg-{:03}", i),
                "adp-pg",
                &format!("ver-pg-{:03}", i),
                "tnt-pg",
                &format!("2026-02-12T{:02}:00:00Z", 10 + i),
            );
            db.store_provenance_certificate(&cert).await.unwrap();
        }

        let page1 = db
            .list_provenance_certificates("tnt-pg", 2, 0)
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].certificate_id, "cert-pg-004"); // newest first

        let page2 = db
            .list_provenance_certificates("tnt-pg", 2, 2)
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = db
            .list_provenance_certificates("tnt-pg", 2, 4)
            .await
            .unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn test_duplicate_certificate_id_rejected() {
        let db = Db::new_in_memory().await.unwrap();

        let cert = make_cert(
            "cert-dup",
            "adp-001",
            "ver-001",
            "tnt-001",
            "2026-02-12T12:00:00Z",
        );
        db.store_provenance_certificate(&cert).await.unwrap();

        let dup = make_cert(
            "cert-dup",
            "adp-002",
            "ver-002",
            "tnt-002",
            "2026-02-12T13:00:00Z",
        );
        let result = db.store_provenance_certificate(&dup).await;
        assert!(
            result.is_err(),
            "Duplicate certificate_id should be rejected"
        );
    }
}
