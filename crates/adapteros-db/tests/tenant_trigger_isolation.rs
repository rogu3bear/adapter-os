//! PRD-RECT-004: DB Trigger Tenant Isolation Revalidation Tests
//!
//! These tests validate SQLite triggers that enforce tenant isolation for
//! adapter/base-model references introduced in migrations 0131 and 0211.

use adapteros_db::Db;
use std::collections::BTreeMap;
use std::ops::Deref;

struct TestDb {
    db: Db,
    _path: tempfile::TempPath,
}

impl Deref for TestDb {
    type Target = Db;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

async fn new_test_db() -> TestDb {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    std::fs::create_dir_all("var/tmp")
        .expect("Failed to create var/tmp for tenant trigger isolation tests");
    let temp_file = tempfile::Builder::new()
        .prefix("tenant-trigger-")
        .suffix(".sqlite3")
        .tempfile_in("var/tmp")
        .expect("Failed to create temp sqlite file for tenant trigger isolation tests");
    let path = temp_file.path().to_path_buf();
    let temp_path = temp_file.into_temp_path();

    let db = Db::connect(&path.to_string_lossy())
        .await
        .expect("Failed to create sqlite database for tenant trigger isolation test");
    db.migrate()
        .await
        .expect("Failed to migrate sqlite database for tenant trigger isolation test");

    TestDb {
        db,
        _path: temp_path,
    }
}

async fn setup_tenants(db: &Db) -> (String, String) {
    let tenant_a = "tenant-a-isolation-test";
    let tenant_b = "tenant-b-isolation-test";

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_a)
        .bind("Tenant A")
        .execute(db.pool_result().unwrap())
        .await
        .expect("Failed to create tenant A for trigger isolation test");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_b)
        .bind("Tenant B")
        .execute(db.pool_result().unwrap())
        .await
        .expect("Failed to create tenant B for trigger isolation test");

    (tenant_a.to_string(), tenant_b.to_string())
}

async fn create_test_repo(db: &Db, tenant_id: &str, name: &str) -> String {
    let repo_id = format!("repo-{}-{}", tenant_id, uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name, default_branch) VALUES (?, ?, ?, ?)",
    )
    .bind(&repo_id)
    .bind(tenant_id)
    .bind(name)
    .bind("main")
    .execute(db.pool_result().unwrap())
    .await
    .expect("Failed to create test adapter repository for trigger isolation test");

    repo_id
}

async fn create_test_version(
    db: &Db,
    repo_id: &str,
    tenant_id: &str,
    version: &str,
) -> Result<String, sqlx::Error> {
    let version_id = format!("ver-{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO adapter_versions (id, repo_id, tenant_id, version, branch, release_state)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&version_id)
    .bind(repo_id)
    .bind(tenant_id)
    .bind(version)
    .bind("main")
    .bind("draft")
    .execute(db.pool_result().unwrap())
    .await?;

    Ok(version_id)
}

async fn create_test_dataset(db: &Db, tenant_id: &str, name: &str) -> String {
    let dataset_id = format!("ds-{}-{}", tenant_id, uuid::Uuid::new_v4());
    let hash_b3 = blake3::hash(dataset_id.as_bytes()).to_hex().to_string();

    sqlx::query(
        "INSERT INTO training_datasets (id, tenant_id, name, format, hash_b3, storage_path)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&dataset_id)
    .bind(tenant_id)
    .bind(name)
    .bind("jsonl")
    .bind(&hash_b3)
    .bind(format!("var/test/{}", dataset_id))
    .execute(db.pool_result().unwrap())
    .await
    .expect("create dataset");

    dataset_id
}

async fn create_test_adapter(db: &Db, tenant_id: &str, name: &str) -> String {
    let adapter_id = format!("adapter-{}-{}", tenant_id, uuid::Uuid::new_v4());
    let hash_b3 = blake3::hash(adapter_id.as_bytes()).to_hex().to_string();

    sqlx::query(
        "INSERT INTO adapters (id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, adapter_id, lifecycle_state, active)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&adapter_id)
    .bind(tenant_id)
    .bind(name)
    .bind("persistent")
    .bind(&hash_b3)
    .bind(16)
    .bind(1.0f64)
    .bind("[]")
    .bind(&adapter_id)
    .bind("active")
    .bind(1)
    .execute(db.pool_result().unwrap())
    .await
    .expect("create adapter");

    adapter_id
}

async fn create_test_git_repo(db: &Db, repo_id: &str) -> Result<(), sqlx::Error> {
    let git_id = format!("git-{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT OR IGNORE INTO git_repositories (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&git_id)
    .bind(repo_id)
    .bind(format!("var/test/{}", repo_id))
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind("active")
    .bind("test-user")
    .execute(db.pool_result().unwrap())
    .await?;

    Ok(())
}

async fn create_test_training_job(
    db: &Db,
    tenant_id: &str,
    repo_id: &str,
    dataset_id: &str,
) -> Result<String, sqlx::Error> {
    create_test_git_repo(db, repo_id).await?;

    let job_id = format!("job-{}-{}", tenant_id, uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, dataset_id, status, training_config_json, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&job_id)
    .bind(repo_id)
    .bind(tenant_id)
    .bind(dataset_id)
    .bind("pending")
    .bind("{}")
    .bind("{}")
    .bind("test-user")
    .execute(db.pool_result().unwrap())
    .await?;

    Ok(job_id)
}

fn normalize_sql(sql: &str) -> String {
    sql.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[tokio::test]
async fn test_tenant_trigger_isolation_adapter_base_model_trigger_presence() {
    let db = new_test_db().await;

    let trigger_rows: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT name, tbl_name, sql FROM sqlite_master WHERE type = 'trigger'")
            .fetch_all(db.pool_result().unwrap())
            .await
            .expect("fetch triggers");

    let triggers: BTreeMap<String, (String, String)> = trigger_rows
        .into_iter()
        .map(|(name, table, sql)| (name, (table, sql.unwrap_or_default())))
        .collect();

    let expected_triggers: Vec<(&str, &str, &[&str])> = vec![
        (
            "trg_adapter_versions_repo_tenant_match_insert",
            "adapter_versions",
            &[
                "adapter_repositories",
                "new.repo_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapter_versions_repo_tenant_match_update_repo",
            "adapter_versions",
            &[
                "adapter_repositories",
                "new.repo_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapter_versions_repo_tenant_match_update_tenant",
            "adapter_versions",
            &[
                "adapter_repositories",
                "new.repo_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapters_primary_dataset_tenant_check",
            "adapters",
            &[
                "training_datasets",
                "new.primary_dataset_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapters_primary_dataset_tenant_check_update",
            "adapters",
            &[
                "training_datasets",
                "new.primary_dataset_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapters_eval_dataset_tenant_check",
            "adapters",
            &[
                "training_datasets",
                "new.eval_dataset_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapters_eval_dataset_tenant_check_update",
            "adapters",
            &[
                "training_datasets",
                "new.eval_dataset_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapters_training_job_tenant_check",
            "adapters",
            &[
                "repository_training_jobs",
                "new.training_job_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapters_training_job_tenant_check_update",
            "adapters",
            &[
                "repository_training_jobs",
                "new.training_job_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_pinned_adapters_tenant_check",
            "pinned_adapters",
            &["adapters", "new.adapter_pk", "new.tenant_id", "raise(abort"],
        ),
        (
            "trg_pinned_adapters_tenant_check_update",
            "pinned_adapters",
            &["adapters", "new.adapter_pk", "new.tenant_id", "raise(abort"],
        ),
        (
            "trg_dataset_adapter_links_tenant_check",
            "dataset_adapter_links",
            &[
                "adapters",
                "training_datasets",
                "new.adapter_id",
                "new.dataset_id",
                "raise(abort",
            ],
        ),
        (
            "trg_dataset_adapter_links_tenant_check_update",
            "dataset_adapter_links",
            &[
                "adapters",
                "training_datasets",
                "new.adapter_id",
                "new.dataset_id",
                "raise(abort",
            ],
        ),
        (
            "trg_evidence_entries_adapter_tenant_check",
            "evidence_entries",
            &["adapters", "new.adapter_id", "new.tenant_id", "raise(abort"],
        ),
        (
            "trg_evidence_entries_adapter_tenant_check_update",
            "evidence_entries",
            &["adapters", "new.adapter_id", "new.tenant_id", "raise(abort"],
        ),
        // Migration 0131: Additional tenant isolation triggers
        (
            "trg_chat_sessions_stack_tenant_check",
            "chat_sessions",
            &[
                "adapter_stacks",
                "new.stack_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_chat_sessions_stack_tenant_check_update",
            "chat_sessions",
            &[
                "adapter_stacks",
                "new.stack_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_chat_sessions_collection_tenant_check",
            "chat_sessions",
            &[
                "document_collections",
                "new.collection_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_chat_sessions_collection_tenant_check_update",
            "chat_sessions",
            &[
                "document_collections",
                "new.collection_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_routing_decisions_stack_tenant_check",
            "routing_decisions",
            &[
                "adapter_stacks",
                "new.stack_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_routing_decisions_stack_tenant_check_update",
            "routing_decisions",
            &[
                "adapter_stacks",
                "new.stack_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_training_jobs_collection_tenant_check",
            "repository_training_jobs",
            &[
                "document_collections",
                "new.collection_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_training_jobs_collection_tenant_check_update",
            "repository_training_jobs",
            &[
                "document_collections",
                "new.collection_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_training_jobs_dataset_tenant_check",
            "repository_training_jobs",
            &[
                "training_datasets",
                "new.dataset_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_training_jobs_dataset_tenant_check_update",
            "repository_training_jobs",
            &[
                "training_datasets",
                "new.dataset_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        // Migration 0215: Dataset files/statistics triggers
        (
            "trg_dataset_files_tenant_check",
            "dataset_files",
            &[
                "training_datasets",
                "new.dataset_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_dataset_statistics_tenant_check",
            "dataset_statistics",
            &[
                "training_datasets",
                "new.dataset_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        // Migration 0223: Adapter stacks cross-tenant validation
        (
            "trg_adapter_stacks_cross_tenant_insert",
            "adapter_stacks",
            &["json_each", "adapters", "new.tenant_id", "raise(abort"],
        ),
        (
            "trg_adapter_stacks_cross_tenant_update_adapters",
            "adapter_stacks",
            &["json_each", "adapters", "new.tenant_id", "raise(abort"],
        ),
        (
            "trg_adapter_stacks_cross_tenant_update_tenant",
            "adapter_stacks",
            &["json_each", "adapters", "new.tenant_id", "raise(abort"],
        ),
        // Migration 0224: Training jobs adapter tenant match
        (
            "trg_training_jobs_adapter_tenant_match_insert",
            "repository_training_jobs",
            &["adapters", "new.adapter_id", "new.tenant_id", "raise(abort"],
        ),
        (
            "trg_training_jobs_adapter_tenant_match_update_adapter",
            "repository_training_jobs",
            &["adapters", "new.adapter_id", "new.tenant_id", "raise(abort"],
        ),
        (
            "trg_training_jobs_adapter_tenant_match_update_tenant",
            "repository_training_jobs",
            &["adapters", "new.adapter_id", "new.tenant_id", "raise(abort"],
        ),
        // Migration 0226: Base model tenant guards (covered in extended tests but included for completeness)
        (
            "trg_adapters_base_model_tenant_check",
            "adapters",
            &[
                "models",
                "new.base_model_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapters_base_model_tenant_check_update",
            "adapters",
            &[
                "models",
                "new.base_model_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapter_repositories_base_model_tenant_check",
            "adapter_repositories",
            &[
                "models",
                "new.base_model_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
        (
            "trg_adapter_repositories_base_model_tenant_check_update",
            "adapter_repositories",
            &[
                "models",
                "new.base_model_id",
                "new.tenant_id",
                "raise(abort",
            ],
        ),
    ];

    let mut missing = Vec::new();
    let mut mismatched = Vec::new();

    for (name, table, patterns) in expected_triggers {
        match triggers.get(name) {
            None => missing.push(name.to_string()),
            Some((trigger_table, sql)) => {
                if trigger_table != table {
                    mismatched.push(format!(
                        "{name} table mismatch: expected {table}, found {trigger_table}"
                    ));
                }

                let normalized = normalize_sql(sql);
                for pattern in patterns {
                    let normalized_pattern = normalize_sql(pattern);
                    if !normalized.contains(&normalized_pattern) {
                        mismatched.push(format!("{name} missing pattern: {pattern}"));
                    }
                }
            }
        }
    }

    match (
        triggers.get("trg_adapter_version_history_tenant_match"),
        triggers.get("trg_adapter_version_history_tenant_check"),
    ) {
        (Some((table, sql)), _) => {
            if table != "adapter_version_history" {
                mismatched.push(format!(
                    "trg_adapter_version_history_tenant_match table mismatch: expected adapter_version_history, found {table}"
                ));
            }

            let normalized = normalize_sql(sql);
            for pattern in [
                "adapter_versions",
                "new.version_id",
                "new.tenant_id",
                "raise(abort",
            ] {
                let normalized_pattern = normalize_sql(pattern);
                if !normalized.contains(&normalized_pattern) {
                    mismatched.push(format!(
                        "trg_adapter_version_history_tenant_match missing pattern: {pattern}"
                    ));
                }
            }
        }
        (None, Some((table, sql))) => {
            if table != "adapter_version_history" {
                mismatched.push(format!(
                    "trg_adapter_version_history_tenant_check table mismatch: expected adapter_version_history, found {table}"
                ));
            }

            let normalized = normalize_sql(sql);
            for pattern in ["adapters", "new.adapter_pk", "new.tenant_id", "raise(abort"] {
                let normalized_pattern = normalize_sql(pattern);
                if !normalized.contains(&normalized_pattern) {
                    mismatched.push(format!(
                        "trg_adapter_version_history_tenant_check missing pattern: {pattern}"
                    ));
                }
            }
        }
        (None, None) => missing.push("adapter_version_history tenant guard trigger".to_string()),
    }

    assert!(
        missing.is_empty() && mismatched.is_empty(),
        "Missing adapter/base-model tenant triggers: {:?}. Trigger mismatches: {:?}",
        missing,
        mismatched
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_adapter_versions_cross_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;
    let result = create_test_version(&db, &repo_a, &tenant_b, "1.0.0").await;

    assert!(
        result.is_err(),
        "Cross-tenant adapter_versions insert should be rejected"
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Tenant mismatch") || err.contains("adapter_versions.tenant_id must match"),
        "Unexpected error message: {err}"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_allows_adapter_versions_same_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;
    let result = create_test_version(&db, &repo_a, &tenant_a, "1.0.0").await;

    assert!(
        result.is_ok(),
        "Same-tenant adapter_versions insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_adapter_versions_cross_tenant_repo_update() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;
    let repo_b = create_test_repo(&db, &tenant_b, "Repo B").await;
    let version_id = create_test_version(&db, &repo_a, &tenant_a, "1.0.0")
        .await
        .expect("create version");

    let result = sqlx::query("UPDATE adapter_versions SET repo_id = ? WHERE id = ?")
        .bind(&repo_b)
        .bind(&version_id)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant repo_id update should be rejected"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_adapter_versions_cross_tenant_tenant_update() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;
    let version_id = create_test_version(&db, &repo_a, &tenant_a, "1.0.0")
        .await
        .expect("create version");

    let result = sqlx::query("UPDATE adapter_versions SET tenant_id = ? WHERE id = ?")
        .bind(&tenant_b)
        .bind(&version_id)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant tenant_id update should be rejected"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_adapters_cross_tenant_training_job() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let dataset_b = create_test_dataset(&db, &tenant_b, "Dataset B").await;
    let job_b = create_test_training_job(&db, &tenant_b, "repo-b", &dataset_b)
        .await
        .expect("create training job");
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;

    let result = sqlx::query("UPDATE adapters SET training_job_id = ? WHERE id = ?")
        .bind(&job_b)
        .bind(&adapter_a)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant training_job_id update should be rejected"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_allows_adapters_same_tenant_training_job() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let dataset_a = create_test_dataset(&db, &tenant_a, "Dataset A").await;
    let job_a = create_test_training_job(&db, &tenant_a, "repo-a", &dataset_a)
        .await
        .expect("create training job");
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;

    let result = sqlx::query("UPDATE adapters SET training_job_id = ? WHERE id = ?")
        .bind(&job_a)
        .bind(&adapter_a)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_ok(),
        "Same-tenant training_job_id update should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_dataset_adapter_links_cross_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let dataset_a = create_test_dataset(&db, &tenant_a, "Dataset A").await;
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;

    let link_id = format!("link-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO dataset_adapter_links (id, dataset_id, adapter_id, link_type, tenant_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&link_id)
    .bind(&dataset_a)
    .bind(&adapter_b)
    .bind("training")
    .bind(&tenant_a)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant dataset_adapter_links insert should be rejected"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_allows_dataset_adapter_links_same_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let dataset_a = create_test_dataset(&db, &tenant_a, "Dataset A").await;
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;

    let link_id = format!("link-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO dataset_adapter_links (id, dataset_id, adapter_id, link_type, tenant_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&link_id)
    .bind(&dataset_a)
    .bind(&adapter_a)
    .bind("training")
    .bind(&tenant_a)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant dataset_adapter_links insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_dataset_adapter_links_cross_tenant_update() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let dataset_a = create_test_dataset(&db, &tenant_a, "Dataset A").await;
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;

    let link_id = format!("link-{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO dataset_adapter_links (id, dataset_id, adapter_id, link_type, tenant_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&link_id)
    .bind(&dataset_a)
    .bind(&adapter_a)
    .bind("training")
    .bind(&tenant_a)
    .execute(db.pool_result().unwrap())
    .await
    .expect("insert link");

    let result = sqlx::query("UPDATE dataset_adapter_links SET adapter_id = ? WHERE id = ?")
        .bind(&adapter_b)
        .bind(&link_id)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant dataset_adapter_links update should be rejected"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_evidence_entries_cross_tenant_adapter_insert() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;

    let entry_id = format!("evidence-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO evidence_entries (id, adapter_id, evidence_type, reference, tenant_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&adapter_b)
    .bind("doc")
    .bind("ref")
    .bind(&tenant_a)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant evidence_entries insert should be rejected"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_allows_evidence_entries_same_tenant_adapter_insert() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;

    let entry_id = format!("evidence-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO evidence_entries (id, adapter_id, evidence_type, reference, tenant_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&adapter_a)
    .bind("doc")
    .bind("ref")
    .bind(&tenant_a)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant evidence_entries insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_evidence_entries_cross_tenant_adapter_update() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;

    let entry_id = format!("evidence-{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO evidence_entries (id, adapter_id, evidence_type, reference, tenant_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&adapter_a)
    .bind("doc")
    .bind("ref")
    .bind(&tenant_a)
    .execute(db.pool_result().unwrap())
    .await
    .expect("insert evidence entry");

    let result = sqlx::query("UPDATE evidence_entries SET adapter_id = ? WHERE id = ?")
        .bind(&adapter_b)
        .bind(&entry_id)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant evidence_entries update should be rejected"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_rejects_adapter_version_history_cross_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;
    let version_id = create_test_version(&db, &repo_a, &tenant_a, "1.0.0")
        .await
        .expect("create version");

    let history_id = format!("hist-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO adapter_version_history (id, repo_id, tenant_id, version_id, branch, new_state)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&history_id)
    .bind(&repo_a)
    .bind(&tenant_b)
    .bind(&version_id)
    .bind("main")
    .bind("draft")
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant adapter_version_history insert should be rejected"
    );
}

#[tokio::test]
async fn test_tenant_trigger_isolation_allows_adapter_version_history_same_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;
    let version_id = create_test_version(&db, &repo_a, &tenant_a, "1.0.0")
        .await
        .expect("create version");

    let history_id = format!("hist-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO adapter_version_history (id, repo_id, tenant_id, version_id, branch, new_state)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&history_id)
    .bind(&repo_a)
    .bind(&tenant_a)
    .bind(&version_id)
    .bind("main")
    .bind("draft")
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant adapter_version_history insert should succeed: {:?}",
        result.err()
    );
}

// ============================================================================
// Migration 0223: adapter_stacks cross-tenant validation tests
// ============================================================================

async fn create_test_stack(db: &Db, tenant_id: &str, suffix: &str, adapter_ids: &[&str]) -> String {
    let stack_id = format!("stack-{}-{}", tenant_id, uuid::Uuid::new_v4());
    let adapter_ids_json = serde_json::to_string(adapter_ids).unwrap();
    // Stack names must match format: stack.{namespace}[.{identifier}]
    let stack_name = format!("stack.test.{}", suffix.replace(' ', "-").to_lowercase());

    sqlx::query(
        "INSERT INTO adapter_stacks (id, tenant_id, name, adapter_ids_json, version, lifecycle_state)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&stack_id)
    .bind(tenant_id)
    .bind(&stack_name)
    .bind(&adapter_ids_json)
    .bind("1.0.0")
    .bind("active")
    .execute(db.pool_result().unwrap())
    .await
    .expect("create stack");

    stack_id
}

#[tokio::test]
async fn test_adapter_stacks_cross_tenant_insert_rejected() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create adapter in tenant B
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;

    // Try to create stack in tenant A with adapter from tenant B
    let stack_id = format!("stack-{}", uuid::Uuid::new_v4());
    let adapter_ids_json = serde_json::to_string(&[&adapter_b]).unwrap();

    let result = sqlx::query(
        "INSERT INTO adapter_stacks (id, tenant_id, name, adapter_ids_json, version, lifecycle_state)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&stack_id)
    .bind(&tenant_a)
    .bind("stack.test.cross-tenant")
    .bind(&adapter_ids_json)
    .bind("1.0.0")
    .bind("active")
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Stack with cross-tenant adapter should be rejected"
    );
}

#[tokio::test]
async fn test_adapter_stacks_same_tenant_insert_allowed() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    // Create adapter in tenant A
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;

    // Create stack in tenant A with adapter from tenant A
    let stack_id = format!("stack-{}", uuid::Uuid::new_v4());
    let adapter_ids_json = serde_json::to_string(&[&adapter_a]).unwrap();

    let result = sqlx::query(
        "INSERT INTO adapter_stacks (id, tenant_id, name, adapter_ids_json, version, lifecycle_state)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&stack_id)
    .bind(&tenant_a)
    .bind("stack.test.same-tenant")
    .bind(&adapter_ids_json)
    .bind("1.0.0")
    .bind("active")
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_ok(),
        "Stack with same-tenant adapter should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_adapter_stacks_cross_tenant_update_rejected() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create adapters in both tenants
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;

    // Create stack with same-tenant adapter
    let stack_id = create_test_stack(&db, &tenant_a, "Update Test Stack", &[&adapter_a]).await;

    // Try to update stack to include cross-tenant adapter
    let cross_tenant_json = serde_json::to_string(&[&adapter_b]).unwrap();
    let result = sqlx::query("UPDATE adapter_stacks SET adapter_ids_json = ? WHERE id = ?")
        .bind(&cross_tenant_json)
        .bind(&stack_id)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_err(),
        "Stack update with cross-tenant adapter should be rejected"
    );
}

// ============================================================================
// Migration 0224: training_jobs adapter tenant match tests
// ============================================================================

#[tokio::test]
async fn test_training_jobs_adapter_cross_tenant_insert_rejected() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create adapter in tenant B
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;
    let dataset_a = create_test_dataset(&db, &tenant_a, "Dataset A").await;

    // Try to create training job in tenant A with adapter from tenant B
    let job_id = format!("job-{}", uuid::Uuid::new_v4());
    let repo_id = format!("repo-{}", uuid::Uuid::new_v4());

    // Create repo first
    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name, default_branch) VALUES (?, ?, ?, ?)",
    )
    .bind(&repo_id)
    .bind(&tenant_a)
    .bind("Test Repo")
    .bind("main")
    .execute(db.pool_result().unwrap())
    .await
    .expect("create repo");

    // Create git repo for FK
    let git_id = format!("git-{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT OR IGNORE INTO git_repositories (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&git_id)
    .bind(&repo_id)
    .bind("var/test/repo")
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind("active")
    .bind("test-user")
    .execute(db.pool_result().unwrap())
    .await
    .expect("create git repo");

    let result = sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, dataset_id, adapter_id, status, training_config_json, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&job_id)
    .bind(&repo_id)
    .bind(&tenant_a)
    .bind(&dataset_a)
    .bind(&adapter_b) // Cross-tenant adapter
    .bind("pending")
    .bind("{}")
    .bind("{}")
    .bind("test-user")
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Training job with cross-tenant adapter should be rejected"
    );
}

#[tokio::test]
async fn test_training_jobs_adapter_same_tenant_insert_allowed() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    // Create adapter in tenant A
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;
    let dataset_a = create_test_dataset(&db, &tenant_a, "Dataset A").await;

    let job_id = format!("job-{}", uuid::Uuid::new_v4());
    let repo_id = format!("repo-{}", uuid::Uuid::new_v4());

    // Create repo first
    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name, default_branch) VALUES (?, ?, ?, ?)",
    )
    .bind(&repo_id)
    .bind(&tenant_a)
    .bind("Test Repo Same Tenant")
    .bind("main")
    .execute(db.pool_result().unwrap())
    .await
    .expect("create repo");

    // Create git repo for FK
    let git_id = format!("git-{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT OR IGNORE INTO git_repositories (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&git_id)
    .bind(&repo_id)
    .bind("var/test/repo-same")
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind("active")
    .bind("test-user")
    .execute(db.pool_result().unwrap())
    .await
    .expect("create git repo");

    let result = sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, dataset_id, adapter_id, status, training_config_json, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&job_id)
    .bind(&repo_id)
    .bind(&tenant_a)
    .bind(&dataset_a)
    .bind(&adapter_a) // Same-tenant adapter
    .bind("pending")
    .bind("{}")
    .bind("{}")
    .bind("test-user")
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_ok(),
        "Training job with same-tenant adapter should succeed: {:?}",
        result.err()
    );
}

// ============================================================================
// Migration 0131: chat_sessions and routing_decisions tenant isolation tests
// ============================================================================

async fn create_test_collection(db: &Db, tenant_id: &str, name: &str) -> String {
    let collection_id = format!("col-{}-{}", tenant_id, uuid::Uuid::new_v4());

    sqlx::query("INSERT INTO document_collections (id, tenant_id, name) VALUES (?, ?, ?)")
        .bind(&collection_id)
        .bind(tenant_id)
        .bind(name)
        .execute(db.pool_result().unwrap())
        .await
        .expect("create collection");

    collection_id
}

#[tokio::test]
async fn test_chat_sessions_cross_tenant_stack_rejected() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create stack in tenant B
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;
    let stack_b = create_test_stack(&db, &tenant_b, "Stack B", &[&adapter_b]).await;

    // Try to create chat session in tenant A with stack from tenant B
    let session_id = format!("session-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO chat_sessions (id, tenant_id, stack_id, title, model_id, messages_json)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(&tenant_a)
    .bind(&stack_b) // Cross-tenant stack
    .bind("Test Session")
    .bind("test-model")
    .bind("[]")
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Chat session with cross-tenant stack should be rejected"
    );
}

#[tokio::test]
async fn test_chat_sessions_cross_tenant_collection_rejected() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create collection in tenant B
    let collection_b = create_test_collection(&db, &tenant_b, "Collection B").await;

    // Try to create chat session in tenant A with collection from tenant B
    let session_id = format!("session-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO chat_sessions (id, tenant_id, collection_id, title, model_id, messages_json)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(&tenant_a)
    .bind(&collection_b) // Cross-tenant collection
    .bind("Test Session")
    .bind("test-model")
    .bind("[]")
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Chat session with cross-tenant collection should be rejected"
    );
}

#[tokio::test]
async fn test_routing_decisions_cross_tenant_stack_rejected() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create stack in tenant B
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;
    let stack_b = create_test_stack(&db, &tenant_b, "Stack B for Routing", &[&adapter_b]).await;

    // Try to create routing decision in tenant A with stack from tenant B
    let decision_id = format!("decision-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO routing_decisions (id, tenant_id, stack_id, input_hash, decision_json, latency_ms)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&decision_id)
    .bind(&tenant_a)
    .bind(&stack_b) // Cross-tenant stack
    .bind("abcd1234")
    .bind("{}")
    .bind(100)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Routing decision with cross-tenant stack should be rejected"
    );
}
