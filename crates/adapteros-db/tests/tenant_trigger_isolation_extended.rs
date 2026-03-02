//! Additional tests for base_model_id tenant isolation
#![allow(clippy::redundant_pub_crate)]

use adapteros_db::Db;

async fn new_test_db() -> Db {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    Db::new_in_memory()
        .await
        .expect("Failed to create in-memory database for extended tenant trigger isolation test")
}

async fn setup_tenants(db: &Db) -> (String, String) {
    let tenant_a = "tenant-a-isolation-test";
    let tenant_b = "tenant-b-isolation-test";

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_a)
        .bind("Tenant A")
        .execute(db.pool_result().unwrap())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_b)
        .bind("Tenant B")
        .execute(db.pool_result().unwrap())
        .await
        .expect("create tenant B");

    (tenant_a.to_string(), tenant_b.to_string())
}

async fn create_test_model(db: &Db, tenant_id: &str, name: &str) -> String {
    let model_id = format!("model-{}-{}", tenant_id, uuid::Uuid::new_v4());
    let hash_b3 = blake3::hash(model_id.as_bytes()).to_hex().to_string();
    let config_hash = blake3::hash(format!("config-{}", model_id).as_bytes())
        .to_hex()
        .to_string();
    let tok_hash = blake3::hash(format!("tok-{}", model_id).as_bytes())
        .to_hex()
        .to_string();
    let tok_cfg_hash = blake3::hash(format!("tok-cfg-{}", model_id).as_bytes())
        .to_hex()
        .to_string();

    sqlx::query(
        "INSERT INTO models (id, tenant_id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&model_id)
    .bind(tenant_id)
    .bind(name)
    .bind(&hash_b3)
    .bind(&config_hash)
    .bind(&tok_hash)
    .bind(&tok_cfg_hash)
    .execute(db.pool_result().unwrap())
    .await
    .expect("create model");

    model_id
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
    .expect("create repo");

    repo_id
}

#[tokio::test]
async fn test_base_model_triggers_reject_adapters_cross_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let model_b = create_test_model(&db, &tenant_b, "Model B").await;

    let adapter_id = format!("adapter-{}-{}", tenant_a, uuid::Uuid::new_v4());
    let hash_b3 = blake3::hash(adapter_id.as_bytes()).to_hex().to_string();

    let result = sqlx::query(
        "INSERT INTO adapters (id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, adapter_id, lifecycle_state, active, base_model_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&adapter_id)
    .bind(&tenant_a)
    .bind("Cross-tenant Base Model Test")
    .bind("persistent")
    .bind(&hash_b3)
    .bind(16)
    .bind(1.0f64)
    .bind("[]")
    .bind(&adapter_id)
    .bind("active")
    .bind(1)
    .bind(&model_b)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant adapters.base_model_id insert should be rejected"
    );
}

#[tokio::test]
async fn test_base_model_triggers_allow_adapters_same_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let model_a = create_test_model(&db, &tenant_a, "Model A").await;

    let adapter_id = format!("adapter-{}-{}", tenant_a, uuid::Uuid::new_v4());
    let hash_b3 = blake3::hash(adapter_id.as_bytes()).to_hex().to_string();

    let result = sqlx::query(
        "INSERT INTO adapters (id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, adapter_id, lifecycle_state, active, base_model_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&adapter_id)
    .bind(&tenant_a)
    .bind("Same-tenant Base Model Test")
    .bind("persistent")
    .bind(&hash_b3)
    .bind(16)
    .bind(1.0f64)
    .bind("[]")
    .bind(&adapter_id)
    .bind("active")
    .bind(1)
    .bind(&model_a)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant adapters.base_model_id insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_base_model_triggers_reject_adapters_cross_tenant_update() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let model_b = create_test_model(&db, &tenant_b, "Model B").await;
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;

    let result = sqlx::query("UPDATE adapters SET base_model_id = ? WHERE id = ?")
        .bind(&model_b)
        .bind(&adapter_a)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant adapters.base_model_id update should be rejected"
    );
}

#[tokio::test]
async fn test_base_model_triggers_allow_adapters_same_tenant_update() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let model_a = create_test_model(&db, &tenant_a, "Model A").await;
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;

    let result = sqlx::query("UPDATE adapters SET base_model_id = ? WHERE id = ?")
        .bind(&model_a)
        .bind(&adapter_a)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_ok(),
        "Same-tenant adapters.base_model_id update should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_base_model_triggers_reject_repositories_cross_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let model_b = create_test_model(&db, &tenant_b, "Model B").await;

    let repo_id = format!("repo-{}-{}", tenant_a, uuid::Uuid::new_v4());

    let result = sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name, default_branch, base_model_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&repo_id)
    .bind(&tenant_a)
    .bind("Cross-tenant Repo Base Model Test")
    .bind("main")
    .bind(&model_b)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant adapter_repositories.base_model_id insert should be rejected"
    );
}

#[tokio::test]
async fn test_base_model_triggers_allow_repositories_same_tenant_insert() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let model_a = create_test_model(&db, &tenant_a, "Model A").await;

    let repo_id = format!("repo-{}-{}", tenant_a, uuid::Uuid::new_v4());

    let result = sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name, default_branch, base_model_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&repo_id)
    .bind(&tenant_a)
    .bind("Same-tenant Repo Base Model Test")
    .bind("main")
    .bind(&model_a)
    .execute(db.pool_result().unwrap())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant adapter_repositories.base_model_id insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_base_model_triggers_reject_repositories_cross_tenant_update() {
    let db = new_test_db().await;
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    let model_b = create_test_model(&db, &tenant_b, "Model B").await;
    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;

    let result = sqlx::query("UPDATE adapter_repositories SET base_model_id = ? WHERE id = ?")
        .bind(&model_b)
        .bind(&repo_a)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant adapter_repositories.base_model_id update should be rejected"
    );
}

#[tokio::test]
async fn test_base_model_triggers_allow_repositories_same_tenant_update() {
    let db = new_test_db().await;
    let (tenant_a, _) = setup_tenants(&db).await;

    let model_a = create_test_model(&db, &tenant_a, "Model A").await;
    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;

    let result = sqlx::query("UPDATE adapter_repositories SET base_model_id = ? WHERE id = ?")
        .bind(&model_a)
        .bind(&repo_a)
        .execute(db.pool_result().unwrap())
        .await;

    assert!(
        result.is_ok(),
        "Same-tenant adapter_repositories.base_model_id update should succeed: {:?}",
        result.err()
    );
}
