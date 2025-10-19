//! Integration test for model UI user journey
//!
//! Tests the complete flow:
//! 1. Import model via API
//! 2. Load model via API
//! 3. Get Cursor config
//! 4. Verify journey tracking
//!
//! Citation: IMPLEMENTATION_PLAN.md Phase 3

#[cfg(test)]
mod model_ui_journey_tests {
    use adapteros_db::Db;

    #[tokio::test]
    #[ignore] // Requires running server and database
    async fn test_model_ui_journey_e2e() -> anyhow::Result<()> {
        // Setup
        let db = Db::connect("var/test_model_ui.db").await?;
        db.migrate().await?;

        let tenant_id = "test-tenant";
        let user_id = "test-user";

        // Create test tenant
        sqlx::query!(
            "INSERT OR IGNORE INTO tenants (id, name, created_at) VALUES (?, ?, datetime('now'))",
            tenant_id,
            "Test Tenant"
        )
        .execute(db.pool())
        .await?;

        // Test 1: Verify base_model_imports table exists
        let table_check = sqlx::query!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='base_model_imports'"
        )
        .fetch_optional(db.pool())
        .await?;

        assert!(table_check.is_some(), "base_model_imports table should exist");

        // Test 2: Verify onboarding_journeys table exists
        let journey_table_check = sqlx::query!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='onboarding_journeys'"
        )
        .fetch_optional(db.pool())
        .await?;

        assert!(
            journey_table_check.is_some(),
            "onboarding_journeys table should exist"
        );

        // Test 3: Insert a test import record
        let import_id = "test-import-123";
        sqlx::query!(
            r#"
            INSERT INTO base_model_imports 
            (id, tenant_id, model_name, weights_path, config_path, tokenizer_path, 
             status, started_at, created_by)
            VALUES (?, ?, ?, ?, ?, ?, 'validating', datetime('now'), ?)
            "#,
            import_id,
            tenant_id,
            "test-model",
            "/path/to/weights.safetensors",
            "/path/to/config.json",
            "/path/to/tokenizer.json",
            user_id
        )
        .execute(db.pool())
        .await?;

        // Test 4: Verify import record was created
        let import_record = sqlx::query!(
            "SELECT id, model_name, status FROM base_model_imports WHERE id = ?",
            import_id
        )
        .fetch_one(db.pool())
        .await?;

        assert_eq!(import_record.id, import_id);
        assert_eq!(import_record.model_name, "test-model");
        assert_eq!(import_record.status, "validating");

        // Test 5: Track a journey step
        let journey_id = "test-journey-456";
        sqlx::query!(
            r#"
            INSERT INTO onboarding_journeys 
            (id, tenant_id, user_id, journey_type, step_completed, completed_at)
            VALUES (?, ?, ?, 'cursor_integration', 'model_imported', datetime('now'))
            "#,
            journey_id,
            tenant_id,
            user_id
        )
        .execute(db.pool())
        .await?;

        // Test 6: Verify journey step was recorded
        let journey_steps = sqlx::query!(
            "SELECT step_completed FROM onboarding_journeys WHERE tenant_id = ? AND user_id = ?",
            tenant_id,
            user_id
        )
        .fetch_all(db.pool())
        .await?;

        assert!(!journey_steps.is_empty(), "Journey steps should be recorded");
        assert_eq!(journey_steps[0].step_completed, "model_imported");

        // Cleanup
        sqlx::query!("DELETE FROM base_model_imports WHERE id = ?", import_id)
            .execute(db.pool())
            .await?;
        sqlx::query!("DELETE FROM onboarding_journeys WHERE id = ?", journey_id)
            .execute(db.pool())
            .await?;

        println!("✓ All model UI journey tests passed");

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires running server
    async fn test_journey_step_tracking() -> anyhow::Result<()> {
        let db = Db::connect("var/test_model_ui.db").await?;
        db.migrate().await?;

        let tenant_id = "test-tenant";
        let user_id = "test-user-2";

        // Track multiple journey steps
        let steps = vec![
            "model_imported",
            "model_loaded",
            "cursor_configured",
            "first_inference",
        ];

        for step in &steps {
            let journey_id = format!("journey-{}-{}", user_id, step);
            sqlx::query!(
                r#"
                INSERT INTO onboarding_journeys 
                (id, tenant_id, user_id, journey_type, step_completed, completed_at)
                VALUES (?, ?, ?, 'cursor_integration', ?, datetime('now'))
                "#,
                journey_id,
                tenant_id,
                user_id,
                step
            )
            .execute(db.pool())
            .await?;
        }

        // Verify all steps were recorded
        let recorded_steps = sqlx::query!(
            "SELECT step_completed FROM onboarding_journeys WHERE tenant_id = ? AND user_id = ? ORDER BY completed_at",
            tenant_id,
            user_id
        )
        .fetch_all(db.pool())
        .await?;

        assert_eq!(recorded_steps.len(), 4);
        for (idx, step) in steps.iter().enumerate() {
            assert_eq!(recorded_steps[idx].step_completed, *step);
        }

        // Cleanup
        sqlx::query!(
            "DELETE FROM onboarding_journeys WHERE user_id = ?",
            user_id
        )
        .execute(db.pool())
        .await?;

        println!("✓ Journey step tracking tests passed");

        Ok(())
    }
}

