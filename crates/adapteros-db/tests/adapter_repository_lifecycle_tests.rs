use adapteros_core::{AosError, Result};
use adapteros_db::{CreateRepositoryParams, CreateVersionParams, Db, RepositoryGroup};
use sqlx;

/// Helper to create a test model for FK satisfaction
async fn create_test_model(db: &Db, model_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO models (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3)
         VALUES (?, ?, ?, 'config-hash', 'tok-hash', 'tok-cfg-hash')",
    )
    .bind(model_id)
    .bind(format!("model-{}", model_id))
    .bind(format!("hash-{}", model_id))
    .execute(&*db.pool())
    .await
    .expect("create test model");
}

async fn create_repo(db: &Db, tenant_id: &str, name: &str, base_model_id: Option<&str>) -> String {
    // Create model if base_model_id is provided
    if let Some(model_id) = base_model_id {
        create_test_model(db, model_id).await;
    }
    db.create_adapter_repository(CreateRepositoryParams {
        tenant_id,
        name,
        base_model_id,
        default_branch: None,
        created_by: Some("tester"),
        description: Some("test repo"),
    })
    .await
    .expect("repo created")
}

async fn create_version(
    db: &Db,
    tenant_id: &str,
    repo_id: &str,
    branch: &str,
    version: &str,
    release_state: &str,
) -> String {
    db.create_adapter_version(CreateVersionParams {
        repo_id,
        tenant_id,
        version,
        branch,
        branch_classification: "protected",
        aos_path: None,
        aos_hash: None,
        manifest_schema_version: None,
        parent_version_id: None,
        code_commit_sha: None,
        data_spec_hash: None,
        training_backend: None,
        coreml_used: None,
        coreml_device_type: None,
        dataset_version_ids: None,
        release_state,
        metrics_snapshot_id: None,
        evaluation_summary: None,
        allow_archived: false,
        actor: Some("tester"),
        reason: None,
        train_job_id: None,
    })
    .await
    .expect("version created")
}

/// Create an adapter version with proper dataset linkage (required for promotion tests).
/// This creates a training dataset, version, and links them to the adapter version.
/// The dataset version is marked as valid so the adapter trust_state becomes "allowed".
async fn create_promotable_version(
    db: &Db,
    tenant_id: &str,
    repo_id: &str,
    branch: &str,
    version: &str,
    release_state: &str,
) -> String {
    // Create dataset and version for proper linking
    let dataset_id = db
        .create_training_dataset(
            &format!("ds-{}-{}", repo_id, version),
            None,
            "jsonl",
            &format!("hash-{}", version),
            &format!("var/ds/{}", version),
            None,
        )
        .await
        .expect("dataset created");

    let ds_version_id = db
        .create_training_dataset_version(
            &dataset_id,
            Some(tenant_id),
            Some("v1"),
            &format!("var/ds/{}/v1", version),
            &format!("hash-{}-v1", version),
            None,
            None,
            None,
        )
        .await
        .expect("dataset version created");

    // Mark dataset version as valid so adapter_trust_state becomes "allowed"
    // (required for is_serveable_version to return true)
    db.update_dataset_version_structural_validation(&ds_version_id, "valid", None)
        .await
        .expect("validation update");

    db.create_adapter_version(CreateVersionParams {
        repo_id,
        tenant_id,
        version,
        branch,
        branch_classification: "protected",
        aos_path: None,
        aos_hash: None,
        manifest_schema_version: None,
        parent_version_id: None,
        code_commit_sha: None,
        data_spec_hash: Some(&format!("hash-{}-v1", version)),
        training_backend: Some("coreml_train"),
        coreml_used: Some(false),
        coreml_device_type: None,
        dataset_version_ids: Some(&[ds_version_id]),
        release_state,
        metrics_snapshot_id: None,
        evaluation_summary: None,
        allow_archived: false,
        actor: Some("tester"),
        reason: None,
        train_job_id: None,
    })
    .await
    .expect("version created")
}

#[tokio::test]
async fn repo_creation_defaults_and_empty_versions() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-a", false).await?;

    // Create the model first to satisfy FK constraint
    create_test_model(&db, "base-x").await;

    let repo_id = db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id: &tenant_id,
            name: "demo",
            base_model_id: Some("base-x"),
            default_branch: None,
            created_by: None,
            description: Some("desc"),
        })
        .await?;

    let repo = db
        .get_adapter_repository(&tenant_id, &repo_id)
        .await?
        .expect("repo exists");

    assert_eq!(repo.default_branch, "main");
    assert_eq!(repo.archived, 0);
    assert_eq!(repo.description.as_deref(), Some("desc"));

    let versions = db
        .list_adapter_versions_for_repo(&tenant_id, &repo_id, None, None)
        .await?;
    assert!(
        versions.is_empty(),
        "no versions should be created by default"
    );

    Ok(())
}

#[tokio::test]
async fn list_repositories_grouped_by_base_model() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-b", false).await?;

    let repo_a1 = create_repo(&db, &tenant_id, "a1", Some("base-a")).await;
    let repo_a2 = create_repo(&db, &tenant_id, "a2", Some("base-a")).await;
    let repo_b1 = create_repo(&db, &tenant_id, "b1", Some("base-b")).await;
    let repo_none = create_repo(&db, &tenant_id, "c1", None).await;

    let groups = db.list_adapter_repositories_grouped(&tenant_id).await?;

    fn find_group<'a>(
        id: Option<&str>,
        groups: &'a [RepositoryGroup],
    ) -> Option<&'a RepositoryGroup> {
        groups.iter().find(|g| g.base_model_id.as_deref() == id)
    }

    let base_a = find_group(Some("base-a"), &groups).expect("base-a group");
    assert_eq!(
        base_a.repositories.len(),
        2,
        "should include repos on same base model"
    );
    assert!(base_a
        .repositories
        .iter()
        .any(|r| r.id == repo_a1 || r.id == repo_a2));

    let base_b = find_group(Some("base-b"), &groups).expect("base-b group");
    assert_eq!(base_b.repositories.len(), 1);
    assert_eq!(base_b.repositories[0].id, repo_b1);

    let none_group = find_group(None, &groups).expect("none group");
    assert_eq!(none_group.repositories.len(), 1);
    assert_eq!(none_group.repositories[0].id, repo_none);

    Ok(())
}

#[tokio::test]
async fn list_versions_orders_by_semver_and_filters_state() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-c", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "semver-repo", None).await;

    let _v0 = create_version(&db, &tenant_id, &repo_id, "main", "0.9.0", "ready").await;
    let v1 = create_version(&db, &tenant_id, &repo_id, "main", "1.0.0", "ready").await;
    let v2 = create_version(&db, &tenant_id, &repo_id, "main", "1.0.1", "active").await;

    let versions = db
        .list_adapter_versions_for_repo(&tenant_id, &repo_id, Some("main"), None)
        .await?;
    let ordered_ids: Vec<String> = versions.into_iter().map(|v| v.id).collect();
    assert_eq!(ordered_ids.first(), Some(&v2));
    assert_eq!(ordered_ids.get(1), Some(&v1));

    let ready_only = db
        .list_adapter_versions_for_repo(&tenant_id, &repo_id, Some("main"), Some(&["ready"][..]))
        .await?;
    assert!(ready_only.iter().all(|v| v.release_state == "ready"));
    assert_eq!(ready_only.len(), 2);

    Ok(())
}

#[tokio::test]
async fn resolve_version_prefers_active_then_ready_and_supports_selectors() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-d", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "resolve-repo", None).await;

    // Use promotable versions with proper dataset linkage
    let ready_id =
        create_promotable_version(&db, &tenant_id, &repo_id, "main", "1.0.0", "ready").await;
    let active_id =
        create_promotable_version(&db, &tenant_id, &repo_id, "main", "1.0.1", "active").await;
    let tag_id =
        create_promotable_version(&db, &tenant_id, &repo_id, "dev", "2.0.0-dev", "deprecated")
            .await;

    let resolved_branch = db
        .resolve_adapter_version(&tenant_id, &repo_id, Some("main"))
        .await?
        .expect("found active");
    assert_eq!(resolved_branch.id, active_id);

    // Ready fallback when no active exists on branch
    let _ = db
        .rollback_adapter_branch(
            &tenant_id,
            &repo_id,
            "main",
            &ready_id,
            Some("tester"),
            Some("test_rollback"),
        )
        .await?;
    let resolved_ready = db
        .resolve_adapter_version(&tenant_id, &repo_id, Some("main"))
        .await?
        .expect("found ready");
    assert_eq!(resolved_ready.id, ready_id);

    let resolved_exact = db
        .resolve_adapter_version(&tenant_id, &repo_id, Some("main@1.0.1"))
        .await?
        .expect("exact match");
    assert_eq!(resolved_exact.id, active_id);

    db.upsert_adapter_version_tag(&tenant_id, &tag_id, "canary")
        .await?;

    let resolved_tag = db
        .resolve_adapter_version(&tenant_id, &repo_id, Some("tag:canary"))
        .await?
        .expect("tag lookup");
    assert_eq!(resolved_tag.id, tag_id);

    Ok(())
}

#[tokio::test]
async fn rollback_marks_states_and_writes_history() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-e", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "rollback-repo", None).await;

    // Use promotable versions with proper dataset linkage
    let active_id =
        create_promotable_version(&db, &tenant_id, &repo_id, "main", "1.0.0", "active").await;
    let ready_id =
        create_promotable_version(&db, &tenant_id, &repo_id, "main", "1.1.0", "ready").await;

    db.rollback_adapter_branch(
        &tenant_id,
        &repo_id,
        "main",
        &ready_id,
        Some("tester"),
        Some("test_rollback"),
    )
    .await?;

    let active_after = db
        .get_adapter_version(&tenant_id, &ready_id)
        .await?
        .expect("target exists");
    assert_eq!(active_after.release_state, "active");

    let deprecated_after = db
        .get_adapter_version(&tenant_id, &active_id)
        .await?
        .expect("previous exists");
    assert_eq!(deprecated_after.release_state, "deprecated");

    // Ensure history rows exist:
    // - 2 from version creation (active + ready)
    // - 2 from rollback (deprecate old + activate new)
    let history_count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM adapter_version_history WHERE repo_id = ?")
            .bind(&repo_id)
            .fetch_one(&*db.pool())
            .await
            .unwrap();
    assert_eq!(history_count, 4);

    Ok(())
}

#[tokio::test]
async fn rollback_failed_promotion_restores_previous_active() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-e2", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "rollback-failed", None).await;

    // Simulate prior active that was deprecated during promotion.
    let previous_id =
        create_version(&db, &tenant_id, &repo_id, "main", "1.0.0", "deprecated").await;
    // Simulate promoted version that later fails post-deployment checks.
    let failed_id = create_version(&db, &tenant_id, &repo_id, "main", "1.1.0", "active").await;

    let restored = db
        .rollback_failed_promotion(
            &tenant_id,
            &repo_id,
            "main",
            &failed_id,
            Some("tester"),
            Some("post_deploy_validation_failed"),
        )
        .await?;

    assert_eq!(restored.as_deref(), Some(previous_id.as_str()));

    let failed = db
        .get_adapter_version(&tenant_id, &failed_id)
        .await?
        .expect("failed version exists");
    assert_eq!(failed.release_state, "failed");

    let restored_version = db
        .get_adapter_version(&tenant_id, &previous_id)
        .await?
        .expect("restored version exists");
    assert_eq!(restored_version.release_state, "active");

    Ok(())
}

#[tokio::test]
async fn archive_blocks_new_versions_without_override() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-f", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "archive-repo", None).await;

    db.archive_adapter_repository(&tenant_id, &repo_id)
        .await
        .expect("archive repo");

    let err = db
        .create_adapter_version(CreateVersionParams {
            repo_id: &repo_id,
            tenant_id: &tenant_id,
            version: "1.0.0",
            branch: "main",
            branch_classification: "protected",
            aos_path: None,
            aos_hash: None,
            manifest_schema_version: None,
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: None,
            training_backend: None,
            coreml_used: None,
            coreml_device_type: None,
            dataset_version_ids: None,
            release_state: "draft",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: false,
            actor: Some("tester"),
            reason: None,
            train_job_id: None,
        })
        .await
        .expect_err("should block new version");

    assert!(
        matches!(err, AosError::Validation(msg) if msg.contains("archived")),
        "expected archived validation error"
    );

    // Override flag allows creation
    let id = db
        .create_adapter_version(CreateVersionParams {
            repo_id: &repo_id,
            tenant_id: &tenant_id,
            version: "1.0.1",
            branch: "main",
            branch_classification: "protected",
            aos_path: None,
            aos_hash: None,
            manifest_schema_version: None,
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: None,
            training_backend: None,
            coreml_used: None,
            coreml_device_type: None,
            dataset_version_ids: None,
            release_state: "draft",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: true,
            actor: Some("tester"),
            reason: None,
            train_job_id: None,
        })
        .await?;

    assert!(!id.is_empty());

    Ok(())
}

#[tokio::test]
async fn promote_version_marks_active_and_deprecates_previous() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-g", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "promote-repo", None).await;

    // Use promotable versions with proper dataset linkage (required for promotion)
    let active_id =
        create_promotable_version(&db, &tenant_id, &repo_id, "main", "1.0.0", "active").await;
    let ready_id =
        create_promotable_version(&db, &tenant_id, &repo_id, "main", "1.1.0", "ready").await;

    db.promote_adapter_version(
        &tenant_id,
        &repo_id,
        &ready_id,
        Some("tester"),
        Some("promote_test"),
    )
    .await?;

    let previous = db
        .get_adapter_version(&tenant_id, &active_id)
        .await?
        .expect("previous version exists");
    assert_eq!(previous.release_state, "deprecated");

    let promoted = db
        .get_adapter_version(&tenant_id, &ready_id)
        .await?
        .expect("promoted version exists");
    assert_eq!(promoted.release_state, "active");

    Ok(())
}

#[tokio::test]
async fn transition_guards_enforced() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-h", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "guard-repo", None).await;

    let draft_id = create_version(&db, &tenant_id, &repo_id, "main", "0.0.1-draft", "draft").await;

    let err = db
        .set_adapter_version_state_with_metadata(
            &draft_id,
            "active",
            None,
            Some("tester"),
            Some("illegal_promotion"),
            None,
        )
        .await;
    assert!(
        matches!(err, Err(AosError::Validation(_))),
        "draft -> active should be rejected"
    );

    let active_id = create_version(&db, &tenant_id, &repo_id, "main", "1.0.0", "active").await;
    let ready_id = create_version(&db, &tenant_id, &repo_id, "main", "1.1.0", "ready").await;

    let err = db
        .set_adapter_version_state_with_metadata(
            &ready_id,
            "active",
            None,
            Some("tester"),
            Some("second_active"),
            None,
        )
        .await;
    assert!(
        matches!(err, Err(AosError::Validation(msg)) if msg.contains("already has active version")),
        "should prevent multiple active versions per branch"
    );

    // Existing active remains unchanged
    let active = db
        .get_adapter_version(&tenant_id, &active_id)
        .await?
        .expect("active exists");
    assert_eq!(active.release_state, "active");

    Ok(())
}

#[tokio::test]
async fn promotion_blocks_legacy_unpinned_on_protected_branch() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-legacy", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "legacy-repo", None).await;

    // Version has no dataset linkage or data_spec_hash (legacy_unpinned)
    let version_id = create_version(&db, &tenant_id, &repo_id, "main", "0.0.1", "ready").await;

    let err = db
        .promote_adapter_version(
            &tenant_id,
            &repo_id,
            &version_id,
            Some("tester"),
            Some("legacy_unpinned_block"),
        )
        .await
        .expect_err("promotion should fail for legacy_unpinned on protected");

    assert!(matches!(err, AosError::Validation(msg) if msg.contains("sandbox")));

    Ok(())
}

#[tokio::test]
async fn coreml_promotion_requires_dataset_and_device() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-i", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "coreml-repo", None).await;

    let dataset_id = db
        .create_training_dataset("ds", None, "jsonl", "hash-ds", "var/ds", None)
        .await?;
    let ds_version_id = db
        .create_training_dataset_version(
            &dataset_id,
            Some(&tenant_id),
            Some("v1"),
            "var/ds/v1",
            "hash-ds",
            None,
            None,
            None,
        )
        .await?;

    let bad_version = db
        .create_adapter_version(CreateVersionParams {
            repo_id: &repo_id,
            tenant_id: &tenant_id,
            version: "0.1.0",
            branch: "main",
            branch_classification: "protected",
            aos_path: None,
            aos_hash: None,
            manifest_schema_version: None,
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: Some("hash-ds"),
            training_backend: Some("coreml_train"),
            coreml_used: Some(true),
            coreml_device_type: None,
            dataset_version_ids: Some(&[ds_version_id.clone()]),
            release_state: "ready",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: false,
            actor: Some("tester"),
            reason: None,
            train_job_id: None,
        })
        .await?;

    let err = db
        .promote_adapter_version(
            &tenant_id,
            &repo_id,
            &bad_version,
            Some("tester"),
            Some("missing_device"),
        )
        .await
        .expect_err("promotion should fail without device");
    assert!(matches!(err, AosError::Validation(_)));

    let good_version = db
        .create_adapter_version(CreateVersionParams {
            repo_id: &repo_id,
            tenant_id: &tenant_id,
            version: "0.2.0",
            branch: "main",
            branch_classification: "protected",
            aos_path: None,
            aos_hash: None,
            manifest_schema_version: None,
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: Some("hash-ds"),
            training_backend: Some("coreml_train"),
            coreml_used: Some(true),
            coreml_device_type: Some("ane"),
            dataset_version_ids: Some(&[ds_version_id.clone()]),
            release_state: "ready",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: false,
            actor: Some("tester"),
            reason: None,
            train_job_id: None,
        })
        .await?;

    db.upsert_adapter_version_dataset_versions(&tenant_id, &good_version, &[ds_version_id])
        .await?;

    db.promote_adapter_version(
        &tenant_id,
        &repo_id,
        &good_version,
        Some("tester"),
        Some("coreml_ok"),
    )
    .await?;

    let promoted = db
        .get_adapter_version(&tenant_id, &good_version)
        .await?
        .expect("promoted version exists");
    assert_eq!(promoted.release_state, "active");

    Ok(())
}

#[tokio::test]
async fn create_version_persists_dataset_links() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-j", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "dataset-link-repo", None).await;

    let dataset_id = db
        .create_training_dataset("ds", None, "jsonl", "hash-ds", "var/ds", None)
        .await?;
    let ds_version_id = db
        .create_training_dataset_version(
            &dataset_id,
            Some(&tenant_id),
            Some("v1"),
            "var/ds/v1",
            "hash-ds",
            None,
            None,
            None,
        )
        .await?;

    let version_id = db
        .create_adapter_version(CreateVersionParams {
            repo_id: &repo_id,
            tenant_id: &tenant_id,
            version: "0.3.0",
            branch: "main",
            branch_classification: "protected",
            aos_path: None,
            aos_hash: None,
            manifest_schema_version: None,
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: Some("hash-ds"),
            training_backend: Some("coreml_train"),
            coreml_used: Some(false),
            coreml_device_type: None,
            dataset_version_ids: Some(&[ds_version_id.clone()]),
            release_state: "draft",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: false,
            actor: Some("tester"),
            reason: None,
            train_job_id: None,
        })
        .await?;

    let linked = db
        .list_dataset_versions_for_adapter_version(&version_id)
        .await?;
    assert_eq!(linked, vec![ds_version_id]);

    Ok(())
}

#[tokio::test]
async fn adapter_version_captures_trust_snapshot() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("tenant-trust", false).await?;
    let repo_id = create_repo(&db, &tenant_id, "trust-repo", None).await;

    let dataset_id = db
        .create_training_dataset("ds", None, "jsonl", "hash-ds", "var/ds", None)
        .await?;
    let ds_version_id = db
        .create_training_dataset_version_with_id(
            "ver-trust",
            &dataset_id,
            Some(&tenant_id),
            Some("v1"),
            "var/ds/v1",
            "hash-ds",
            None,
            None,
            None,
        )
        .await?;

    // Mark dataset version as valid to drive trust_state = allowed.
    let trust_state = db
        .update_dataset_version_structural_validation(&ds_version_id, "valid", None)
        .await?;

    let version_id = db
        .create_adapter_version(CreateVersionParams {
            repo_id: &repo_id,
            tenant_id: &tenant_id,
            version: "0.4.0",
            branch: "main",
            branch_classification: "protected",
            aos_path: None,
            aos_hash: None,
            manifest_schema_version: None,
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: Some("hash-ds"),
            training_backend: Some("coreml_train"),
            coreml_used: Some(false),
            coreml_device_type: None,
            dataset_version_ids: Some(&[ds_version_id.clone()]),
            release_state: "draft",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: false,
            actor: Some("tester"),
            reason: None,
            train_job_id: None,
        })
        .await?;

    let lineage = db
        .list_dataset_versions_with_trust_for_adapter_version(&version_id)
        .await?;
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].0, ds_version_id);
    assert_eq!(lineage[0].1.as_deref(), Some(trust_state.as_str()));

    Ok(())
}
