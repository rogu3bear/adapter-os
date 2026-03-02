use adapteros_db::Db;

#[tokio::test]
async fn test_plugin_config_crud() -> Result<(), Box<dyn std::error::Error>> {
    // Create in-memory database
    let db = Db::new_in_memory().await?;

    // Test 1: Upsert new plugin config
    db.upsert_plugin_config("code-intelligence", true, Some(r#"{"scan_interval": 300}"#))
        .await?;

    // Test 2: Get plugin config
    let config = db.get_plugin_config("code-intelligence").await?;
    assert!(config.is_some());
    let config = config.unwrap();
    assert_eq!(config.plugin_name, "code-intelligence");
    assert!(config.enabled);
    assert_eq!(
        config.config_json,
        Some(r#"{"scan_interval": 300}"#.to_string())
    );

    // Test 3: Update plugin config
    db.upsert_plugin_config(
        "code-intelligence",
        false,
        Some(r#"{"scan_interval": 600}"#),
    )
    .await?;

    let config = db.get_plugin_config("code-intelligence").await?;
    assert!(config.is_some());
    let config = config.unwrap();
    assert!(!config.enabled);
    assert_eq!(
        config.config_json,
        Some(r#"{"scan_interval": 600}"#.to_string())
    );

    // Test 4: List plugin configs
    db.upsert_plugin_config("telemetry", true, None).await?;
    let configs = db.list_plugin_configs().await?;
    assert_eq!(configs.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_tenant_plugin_enables() -> Result<(), Box<dyn std::error::Error>> {
    // Create in-memory database
    let db = Db::new_in_memory().await?;

    // Seed tenants required for FK constraints
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-123', 'Tenant 123')")
        .execute(db.pool_result()?)
        .await?;
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-456', 'Tenant 456')")
        .execute(db.pool_result()?)
        .await?;

    // Set up global plugin config (disabled by default)
    db.upsert_plugin_config("code-intelligence", false, None)
        .await?;

    // Test 1: Enable plugin for specific tenant
    db.enable_plugin_for_tenant(
        "code-intelligence",
        "tenant-123",
        Some(r#"{"custom": true}"#),
    )
    .await?;

    // Test 2: Check if plugin is enabled for tenant (should override global setting)
    let is_enabled = db
        .is_plugin_enabled_for_tenant("code-intelligence", "tenant-123")
        .await?;
    assert!(is_enabled);

    // Test 3: Check if plugin is enabled for different tenant (should use global setting)
    let is_enabled = db
        .is_plugin_enabled_for_tenant("code-intelligence", "tenant-456")
        .await?;
    assert!(!is_enabled);

    // Test 4: Disable plugin for tenant
    db.disable_plugin_for_tenant("code-intelligence", "tenant-123")
        .await?;
    let is_enabled = db
        .is_plugin_enabled_for_tenant("code-intelligence", "tenant-123")
        .await?;
    assert!(!is_enabled);

    // Test 5: List tenant plugin enables
    db.enable_plugin_for_tenant("telemetry", "tenant-123", None)
        .await?;
    let tenant_plugins = db.list_tenant_plugin_enables("tenant-123").await?;
    assert_eq!(tenant_plugins.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_plugin_global_vs_tenant_precedence() -> Result<(), Box<dyn std::error::Error>> {
    // Create in-memory database
    let db = Db::new_in_memory().await?;

    // Seed tenants required for FK constraints
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-888', 'Tenant 888')")
        .execute(db.pool_result()?)
        .await?;
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-999', 'Tenant 999')")
        .execute(db.pool_result()?)
        .await?;

    // Set up global plugin config (enabled by default)
    db.upsert_plugin_config("code-intelligence", true, None)
        .await?;

    // Test 1: Tenant without override uses global setting
    let is_enabled = db
        .is_plugin_enabled_for_tenant("code-intelligence", "tenant-999")
        .await?;
    assert!(is_enabled);

    // Test 2: Tenant with override uses tenant setting (disabled)
    db.disable_plugin_for_tenant("code-intelligence", "tenant-999")
        .await?;
    let is_enabled = db
        .is_plugin_enabled_for_tenant("code-intelligence", "tenant-999")
        .await?;
    assert!(!is_enabled);

    // Test 3: Tenant with override uses tenant setting (enabled)
    db.enable_plugin_for_tenant("code-intelligence", "tenant-888", None)
        .await?;
    let is_enabled = db
        .is_plugin_enabled_for_tenant("code-intelligence", "tenant-888")
        .await?;
    assert!(is_enabled);

    // Test 4: Global setting change doesn't affect tenant with override
    db.upsert_plugin_config("code-intelligence", false, None)
        .await?;
    let is_enabled = db
        .is_plugin_enabled_for_tenant("code-intelligence", "tenant-888")
        .await?;
    assert!(is_enabled); // Still enabled due to tenant override

    Ok(())
}
