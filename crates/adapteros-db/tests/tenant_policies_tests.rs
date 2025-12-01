//! Tests for tenant policy customizations database operations

#[cfg(test)]
mod tests {
    use adapteros_db::{
        CreateCustomizationRequest, CustomizationStatus, TenantPolicyCustomizationOps,
    };
    use sqlx::SqlitePool;
    use uuid::Uuid;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        
        // Run migration to create tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tenant_policy_customizations (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                base_policy_type TEXT NOT NULL,
                customizations_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'draft',
                submitted_at TEXT,
                reviewed_at TEXT,
                reviewed_by TEXT,
                review_notes TEXT,
                activated_at TEXT,
                created_at TEXT NOT NULL,
                created_by TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata_json TEXT
            );

            CREATE TABLE IF NOT EXISTS tenant_policy_customization_history (
                id TEXT PRIMARY KEY,
                customization_id TEXT NOT NULL,
                action TEXT NOT NULL,
                performed_by TEXT NOT NULL,
                performed_at TEXT NOT NULL,
                old_status TEXT,
                new_status TEXT,
                notes TEXT,
                changes_json TEXT
            );
            "#
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_customization() {
        let pool = setup_test_db().await;
        let ops = TenantPolicyCustomizationOps::new(pool);

        let req = CreateCustomizationRequest {
            tenant_id: "test-tenant".to_string(),
            base_policy_type: "router".to_string(),
            customizations_json: r#"{"k_sparse": 5}"#.to_string(),
            created_by: "test@example.com".to_string(),
            metadata_json: None,
        };

        let result = ops.create_customization(req).await;
        assert!(result.is_ok());

        let customization = result.unwrap();
        assert_eq!(customization.tenant_id, "test-tenant");
        assert_eq!(customization.base_policy_type, "router");
        assert_eq!(customization.status, CustomizationStatus::Draft);
    }

    #[tokio::test]
    async fn test_workflow_transitions() {
        let pool = setup_test_db().await;
        let ops = TenantPolicyCustomizationOps::new(pool);

        // Create draft
        let req = CreateCustomizationRequest {
            tenant_id: "test-tenant".to_string(),
            base_policy_type: "memory".to_string(),
            customizations_json: r#"{"min_headroom_pct": 20}"#.to_string(),
            created_by: "test@example.com".to_string(),
            metadata_json: None,
        };
        let customization = ops.create_customization(req).await.unwrap();
        let id = customization.id.clone();

        // Submit for review
        ops.submit_for_review(&id, "test@example.com").await.unwrap();
        let updated = ops.get_customization(&id).await.unwrap().unwrap();
        assert_eq!(updated.status, CustomizationStatus::PendingReview);
        assert!(updated.submitted_at.is_some());

        // Approve
        ops.approve_customization(&id, "admin@example.com", Some("Looks good")).await.unwrap();
        let approved = ops.get_customization(&id).await.unwrap().unwrap();
        assert_eq!(approved.status, CustomizationStatus::Approved);
        assert_eq!(approved.reviewed_by, Some("admin@example.com".to_string()));
        assert_eq!(approved.review_notes, Some("Looks good".to_string()));

        // Activate
        ops.activate_customization(&id, "admin@example.com").await.unwrap();
        let active = ops.get_customization(&id).await.unwrap().unwrap();
        assert_eq!(active.status, CustomizationStatus::Active);
        assert!(active.activated_at.is_some());
    }

    #[tokio::test]
    async fn test_reject_workflow() {
        let pool = setup_test_db().await;
        let ops = TenantPolicyCustomizationOps::new(pool);

        // Create and submit
        let req = CreateCustomizationRequest {
            tenant_id: "test-tenant".to_string(),
            base_policy_type: "performance".to_string(),
            customizations_json: r#"{"latency_p95_ms": 50}"#.to_string(),
            created_by: "test@example.com".to_string(),
            metadata_json: None,
        };
        let customization = ops.create_customization(req).await.unwrap();
        let id = customization.id.clone();

        ops.submit_for_review(&id, "test@example.com").await.unwrap();

        // Reject
        ops.reject_customization(&id, "compliance@example.com", Some("Invalid configuration")).await.unwrap();
        let rejected = ops.get_customization(&id).await.unwrap().unwrap();
        assert_eq!(rejected.status, CustomizationStatus::Rejected);
        assert_eq!(rejected.reviewed_by, Some("compliance@example.com".to_string()));
        assert_eq!(rejected.review_notes, Some("Invalid configuration".to_string()));
    }

    #[tokio::test]
    async fn test_list_pending_reviews() {
        let pool = setup_test_db().await;
        let ops = TenantPolicyCustomizationOps::new(pool);

        // Create multiple customizations
        for i in 0..3 {
            let req = CreateCustomizationRequest {
                tenant_id: format!("tenant-{}", i),
                base_policy_type: "router".to_string(),
                customizations_json: r#"{"k_sparse": 4}"#.to_string(),
                created_by: format!("user{}@example.com", i),
                metadata_json: None,
            };
            let customization = ops.create_customization(req).await.unwrap();
            ops.submit_for_review(&customization.id, &format!("user{}@example.com", i)).await.unwrap();
        }

        let pending = ops.list_pending_reviews().await.unwrap();
        assert_eq!(pending.len(), 3);
        assert!(pending.iter().all(|c| c.status == CustomizationStatus::PendingReview));
    }

    #[tokio::test]
    async fn test_activate_deactivates_existing() {
        let pool = setup_test_db().await;
        let ops = TenantPolicyCustomizationOps::new(pool);

        // Create and activate first customization
        let req1 = CreateCustomizationRequest {
            tenant_id: "test-tenant".to_string(),
            base_policy_type: "router".to_string(),
            customizations_json: r#"{"k_sparse": 3}"#.to_string(),
            created_by: "test@example.com".to_string(),
            metadata_json: None,
        };
        let c1 = ops.create_customization(req1).await.unwrap();
        ops.submit_for_review(&c1.id, "test@example.com").await.unwrap();
        ops.approve_customization(&c1.id, "admin@example.com", None).await.unwrap();
        ops.activate_customization(&c1.id, "admin@example.com").await.unwrap();

        // Create and activate second customization (same type)
        let req2 = CreateCustomizationRequest {
            tenant_id: "test-tenant".to_string(),
            base_policy_type: "router".to_string(),
            customizations_json: r#"{"k_sparse": 5}"#.to_string(),
            created_by: "test@example.com".to_string(),
            metadata_json: None,
        };
        let c2 = ops.create_customization(req2).await.unwrap();
        ops.submit_for_review(&c2.id, "test@example.com").await.unwrap();
        ops.approve_customization(&c2.id, "admin@example.com", None).await.unwrap();
        ops.activate_customization(&c2.id, "admin@example.com").await.unwrap();

        // First should be deactivated
        let c1_updated = ops.get_customization(&c1.id).await.unwrap().unwrap();
        assert_eq!(c1_updated.status, CustomizationStatus::Draft);

        // Second should be active
        let c2_updated = ops.get_customization(&c2.id).await.unwrap().unwrap();
        assert_eq!(c2_updated.status, CustomizationStatus::Active);
    }

    #[tokio::test]
    async fn test_update_draft_only() {
        let pool = setup_test_db().await;
        let ops = TenantPolicyCustomizationOps::new(pool);

        let req = CreateCustomizationRequest {
            tenant_id: "test-tenant".to_string(),
            base_policy_type: "router".to_string(),
            customizations_json: r#"{"k_sparse": 3}"#.to_string(),
            created_by: "test@example.com".to_string(),
            metadata_json: None,
        };
        let customization = ops.create_customization(req).await.unwrap();
        let id = customization.id.clone();

        // Update should succeed for draft
        ops.update_customization(&id, r#"{"k_sparse": 4}"#, "test@example.com").await.unwrap();
        let updated = ops.get_customization(&id).await.unwrap().unwrap();
        assert_eq!(updated.customizations_json, r#"{"k_sparse": 4}"#);

        // Submit and try to update - should fail
        ops.submit_for_review(&id, "test@example.com").await.unwrap();
        let result = ops.update_customization(&id, r#"{"k_sparse": 5}"#, "test@example.com").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_draft_only() {
        let pool = setup_test_db().await;
        let ops = TenantPolicyCustomizationOps::new(pool);

        // Create draft
        let req = CreateCustomizationRequest {
            tenant_id: "test-tenant".to_string(),
            base_policy_type: "router".to_string(),
            customizations_json: r#"{"k_sparse": 3}"#.to_string(),
            created_by: "test@example.com".to_string(),
            metadata_json: None,
        };
        let customization = ops.create_customization(req).await.unwrap();
        let id = customization.id.clone();

        // Delete should succeed
        ops.delete_customization(&id).await.unwrap();
        let result = ops.get_customization(&id).await.unwrap();
        assert!(result.is_none());

        // Create and submit another
        let req2 = CreateCustomizationRequest {
            tenant_id: "test-tenant".to_string(),
            base_policy_type: "memory".to_string(),
            customizations_json: r#"{"min_headroom_pct": 15}"#.to_string(),
            created_by: "test@example.com".to_string(),
            metadata_json: None,
        };
        let c2 = ops.create_customization(req2).await.unwrap();
        ops.submit_for_review(&c2.id, "test@example.com").await.unwrap();

        // Delete should fail for non-draft
        let result = ops.delete_customization(&c2.id).await;
        assert!(result.is_err());
    }
}

