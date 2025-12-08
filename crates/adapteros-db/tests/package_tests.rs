use adapteros_db::sqlite_backend::SqliteBackend;
use adapteros_db::{
    AdapterStrengthOverride, CreatePackageRequest as DbCreatePackageRequest, CreateStackRequest,
    DatabaseBackend, Db, StorageMode,
};

fn stack_name() -> String {
    "stack.pkg.test".to_string()
}

#[tokio::test]
async fn create_and_fetch_package() {
    let backend = SqliteBackend::new(":memory:").await.unwrap();
    backend.run_migrations().await.unwrap();
    let pool = backend.pool().clone();

    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-a', 'Tenant A')")
        .execute(&pool)
        .await
        .unwrap();

    let db = Db::new(pool, None, StorageMode::SqlOnly);

    let stack_req = CreateStackRequest {
        tenant_id: "tenant-a".to_string(),
        name: stack_name(),
        description: Some("test stack".to_string()),
        adapter_ids: vec!["adapter-1".to_string()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: Some("strict".to_string()),
        routing_determinism_mode: Some("deterministic".to_string()),
    };

    let stack_id = db.insert_stack(&stack_req).await.unwrap();

    let pkg_req = DbCreatePackageRequest {
        tenant_id: "tenant-a".to_string(),
        name: "pkg-alpha".to_string(),
        description: Some("first package".to_string()),
        stack_id: stack_id.clone(),
        tags: Some(vec!["alpha".to_string(), "demo".to_string()]),
        domain: Some("code".to_string()),
        scope_path: Some("repo/".to_string()),
        adapter_strengths: Some(vec![AdapterStrengthOverride {
            adapter_id: "adapter-1".to_string(),
            strength: Some(1.0),
        }]),
        determinism_mode: Some("strict".to_string()),
        routing_determinism_mode: Some("deterministic".to_string()),
    };

    let pkg_id = db.create_package(&pkg_req).await.unwrap();
    let fetched = db
        .get_package("tenant-a", &pkg_id)
        .await
        .unwrap()
        .expect("package should exist");

    assert_eq!(fetched.name, "pkg-alpha");
    assert_eq!(fetched.stack_id, stack_id);
    assert_eq!(fetched.determinism_mode.as_deref(), Some("strict"));
    assert_eq!(
        fetched.routing_determinism_mode,
        Some("deterministic".to_string())
    );
}
