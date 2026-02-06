use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::auth::{hash_password, Claims};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::{AosError, B3Hash};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::sqlx::{self, Executor, Row};
use adapteros_db::{
    policy_audit::AUDIT_CHAIN_DIVERGED_CODE,
    CreateRepositoryParams as CreateAdapterRepositoryParams,
    CreateVersionParams as CreateAdapterVersionParams,
};
use serde_json::json;

const E2E_ENV: &str = "E2E_MODE";
const DEV_BYPASS_ENV: &str = "VITE_ENABLE_DEV_BYPASS";
const PRODUCTION_MODE_ENV: &str = "AOS_PRODUCTION_MODE";

// Deterministic fixture constants shared across endpoints
const TENANT_ID: &str = "tenant-test";
const TENANT_NAME: &str = "Test Tenant";
const TENANT_ID_SECONDARY: &str = "tenant-test-2";
const TENANT_NAME_SECONDARY: &str = "Test Tenant 2";
const MODEL_ID: &str = "model-qwen-test";
const MODEL_NAME: &str = "qwen2.5-7b-test";
const MODEL_ID_SECONDARY: &str = "model-qwen-test-2";
const MODEL_NAME_SECONDARY: &str = "qwen2.5-7b-test-2";
const STACK_ID: &str = "stack-test";
const STACK_NAME: &str = "stack.test";
const STACK_ID_SECONDARY: &str = "stack-test-2";
const STACK_NAME_SECONDARY: &str = "stack.test.2";
const ADAPTER_ID: &str = "adapter-test";
const ADAPTER_NAME: &str = "Test Adapter";
const ADAPTER_ID_SECONDARY: &str = "adapter-test-2";
const ADAPTER_NAME_SECONDARY: &str = "Test Adapter 2";
const E2E_USER_ID: &str = "user-e2e";
const E2E_USER_EMAIL: &str = "test@example.com";
const E2E_USER_NAME: &str = "E2E Test User";
const E2E_USER_PASSWORD: &str = "password";
const POLICY_ACTOR: &str = "seed-fixtures";
const FIXED_TS: &str = "2025-01-01T00:00:00Z";
const FIXED_TS_MS: i64 = 1_735_689_600_000;

const TRACE_ID: &str = "trace-fixture";
const TRACE_REQUEST_ID: &str = "req-fixture";
const TRACE_BACKEND_ID: &str = "coreml";
const TRACE_KERNEL_VERSION: &str = "kernel-fixture-v1";
const TRACE_TOKEN_COUNT: usize = 50;

const DOCUMENT_ID: &str = "doc-fixture";
const DOCUMENT_CHUNK_ID: &str = "chunk-fixture";
const EVIDENCE_ID: &str = "evidence-fixture";

fn flag_enabled(env: &str) -> bool {
    matches!(
        std::env::var(env)
            .map(|v| v.to_ascii_lowercase())
            .as_deref(),
        Ok("1") | Ok("true")
    )
}

/// Check if production mode is enabled via AOS_PRODUCTION_MODE env var.
///
/// SECURITY: Production mode blocks testkit endpoints regardless of other env vars.
fn is_production_mode() -> bool {
    flag_enabled(PRODUCTION_MODE_ENV)
}

fn e2e_env_enabled() -> bool {
    flag_enabled(E2E_ENV) || flag_enabled(DEV_BYPASS_ENV)
}

fn ensure_e2e_mode() -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    // SECURITY: Production mode takes precedence - testkit is NEVER available in production
    if is_production_mode() {
        tracing::warn!(
            "Testkit endpoint called in production mode - rejecting request. \
             Testkit endpoints are disabled when AOS_PRODUCTION_MODE=1"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("testkit unavailable in production")
                    .with_code("TESTKIT_PRODUCTION_BLOCKED")
                    .with_string_details(
                        "Testkit endpoints are disabled in production mode (AOS_PRODUCTION_MODE=1). \
                         This is a security measure to prevent data loss.",
                    ),
            ),
        ));
    }

    if e2e_env_enabled() {
        tracing::warn!(
            e2e_mode = flag_enabled(E2E_ENV),
            dev_bypass = flag_enabled(DEV_BYPASS_ENV),
            "Testkit endpoint enabled - this allows destructive operations (reset, seed)"
        );
        return Ok(());
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(
            ErrorResponse::new("testkit unavailable")
                .with_code("E2E_MODE_DISABLED")
                .with_string_details(
                    "Set E2E_MODE=1 or VITE_ENABLE_DEV_BYPASS=1 to enable testkit endpoints",
                ),
        ),
    ))
}

fn hash_bytes(label: &str) -> [u8; 32] {
    B3Hash::hash(label.as_bytes()).to_bytes()
}

fn map_err(err: impl Into<AosError>) -> (StatusCode, Json<ErrorResponse>) {
    let err = err.into();
    eprintln!("[TESTKIT ERROR] {}", err);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            ErrorResponse::new("testkit error")
                .with_code("TESTKIT_ERROR")
                .with_string_details(err.to_string()),
        ),
    )
}

#[derive(Debug, Serialize)]
pub struct TestkitResetResponse {
    pub status: String,
    pub tables_cleared: usize,
}

#[axum::debug_handler]
pub async fn reset(
    State(state): State<AppState>,
) -> Result<Json<TestkitResetResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;

    let pool = state.db.pool();
    let mut conn = pool.acquire().await.map_err(map_err)?;

    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *conn)
        .await
        .map_err(map_err)?;

    let tables: Vec<String> = sqlx::query("SELECT name FROM sqlite_master WHERE type='table'")
        .map(|row: sqlx::sqlite::SqliteRow| row.get::<String, _>("name"))
        .fetch_all(&mut *conn)
        .await
        .map_err(map_err)?;

    let mut cleared = 0usize;
    for table in tables {
        // Skip SQLite internal tables, migration tracking, FTS virtual tables, and reference data tables
        if table.starts_with("sqlite_")
            || table == "_sqlx_migrations"
            || table.ends_with("_fts")
            || table.ends_with("_fts_config")
            || table.ends_with("_fts_content")
            || table.ends_with("_fts_data")
            || table.ends_with("_fts_docsize")
            || table.ends_with("_fts_idx")
            // Reference data tables required by triggers
            || table == "adapter_states"
            || table == "adapter_scopes"
            || table == "adapter_categories"
            || table == "policy_packs"
        {
            continue;
        }
        // Quote identifier to prevent SQL injection (table names from sqlite_master)
        let stmt = format!("DELETE FROM \"{}\"", table.replace('"', "\"\""));
        sqlx::query(&stmt)
            .execute(&mut *conn)
            .await
            .map_err(map_err)?;
        cleared += 1;
    }

    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *conn)
        .await
        .map_err(map_err)?;

    // Re-run migrations to ensure schema consistency (idempotent)
    state.db.migrate().await.map_err(map_err)?;

    Ok(Json(TestkitResetResponse {
        status: "ok".to_string(),
        tables_cleared: cleared,
    }))
}

#[derive(Debug, Serialize)]
pub struct SeedMinimalResponse {
    pub status: String,
    pub tenant_id: String,
    pub user_id: String,
    pub model_id: String,
    pub adapter_id: String,
    pub stack_id: String,
    pub secondary_tenant_id: String,
    pub secondary_model_id: String,
    pub secondary_adapter_id: String,
    pub secondary_stack_id: String,
}

#[axum::debug_handler]
pub async fn seed_minimal(
    State(state): State<AppState>,
) -> Result<Json<SeedMinimalResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let pool = state.db.pool();
    let pinned_primary =
        serde_json::to_string(&vec![ADAPTER_ID]).unwrap_or_else(|_| "[]".to_string());
    let pinned_secondary =
        serde_json::to_string(&vec![ADAPTER_ID_SECONDARY]).unwrap_or_else(|_| "[]".to_string());

    // Seed tenant (default_stack_id=NULL initially since stack doesn't exist yet; updated later)
    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, itar_flag, status, created_at, updated_at, default_stack_id, default_pinned_adapter_ids, determinism_mode)
        VALUES (?, ?, 0, 'active', ?, ?, NULL, ?, 'strict')
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            status = excluded.status,
            updated_at = excluded.updated_at,
            default_pinned_adapter_ids = excluded.default_pinned_adapter_ids
        "#,
    )
    .bind(TENANT_ID)
    .bind(TENANT_NAME)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .bind(&pinned_primary)
    .execute(pool)
    .await
    .map_err(map_err)?;

    // Seed deterministic admin user
    sqlx::query("DELETE FROM users WHERE email = ? OR id = ?")
        .bind(E2E_USER_EMAIL)
        .bind(E2E_USER_ID)
        .execute(pool)
        .await
        .map_err(map_err)?;

    let pw_hash = hash_password(E2E_USER_PASSWORD).map_err(map_err)?;
    let created_user_id = state
        .db
        .create_user(
            E2E_USER_EMAIL,
            E2E_USER_NAME,
            &pw_hash,
            adapteros_db::users::Role::Admin,
            TENANT_ID,
        )
        .await
        .map_err(map_err)?;

    if created_user_id != E2E_USER_ID {
        sqlx::query("UPDATE users SET id = ? WHERE id = ?")
            .bind(E2E_USER_ID)
            .bind(&created_user_id)
            .execute(pool)
            .await
            .map_err(map_err)?;
    }

    sqlx::query(
        r#"
        UPDATE users SET
            tenant_id = ?,
            role = 'admin',
            display_name = ?,
            pw_hash = ?,
            disabled = 0,
            created_at = ?
        WHERE id = ?
        "#,
    )
    .bind(TENANT_ID)
    .bind(E2E_USER_NAME)
    .bind(&pw_hash)
    .bind(FIXED_TS)
    .bind(E2E_USER_ID)
    .execute(pool)
    .await
    .map_err(map_err)?;

    // Register model with deterministic metadata
    sqlx::query("DELETE FROM models WHERE id = ?")
        .bind(MODEL_ID)
        .execute(pool)
        .await
        .map_err(map_err)?;

    let model_params = ModelRegistrationBuilder::new()
        .name(MODEL_NAME)
        .hash_b3("b3_model_qwen25_7b_test")
        .config_hash_b3("b3_model_config_test")
        .tokenizer_hash_b3("b3_model_tokenizer_test")
        .tokenizer_cfg_hash_b3("b3_model_tokenizer_cfg_test")
        .license_hash_b3(Some("b3_license_test"))
        .metadata_json(Some(
            serde_json::json!({"size_bytes": 1024_i64, "quant": "q4_0"}).to_string(),
        ))
        .build()
        .map_err(map_err)?;

    let inserted_model_id = state
        .db
        .register_model(model_params)
        .await
        .map_err(map_err)?;
    if inserted_model_id != MODEL_ID {
        sqlx::query("UPDATE models SET id = ? WHERE id = ?")
            .bind(MODEL_ID)
            .bind(&inserted_model_id)
            .execute(pool)
            .await
            .map_err(map_err)?;
    }

    sqlx::query(
        r#"
        UPDATE models SET
            tenant_id = ?,
            backend = 'mlx',
            quantization = 'q4_0',
            format = 'safetensors',
            size_bytes = 1024,
            import_status = 'available',
            imported_at = ?,
            imported_by = ?
        WHERE id = ?
        "#,
    )
    .bind(TENANT_ID)
    .bind(FIXED_TS)
    .bind("seed-fixtures")
    .bind(MODEL_ID)
    .execute(pool)
    .await
    .map_err(map_err)?;

    // Register adapter
    sqlx::query("DELETE FROM adapters WHERE adapter_id = ? AND tenant_id = ?")
        .bind(ADAPTER_ID)
        .bind(TENANT_ID)
        .execute(pool)
        .await
        .map_err(map_err)?;

    let adapter_params = AdapterRegistrationBuilder::new()
        .tenant_id(TENANT_ID)
        .adapter_id(ADAPTER_ID)
        .name(ADAPTER_NAME)
        .hash_b3("b3_adapter_seed_test")
        .rank(8)
        .tier("warm")
        .alpha(16.0)
        .lora_strength(Some(1.0))
        .targets_json(r#"["attn.q_proj","attn.v_proj"]"#)
        .category("code")
        .scope("global")
        .base_model_id(Some(MODEL_ID))
        .manifest_schema_version(Some("1.0.0"))
        .content_hash_b3(Some("b3_adapter_content_seed"))
        .metadata_json(Some(
            serde_json::json!({"description": "Seed adapter for Cypress", "owner": "seed-fixtures"})
                .to_string(),
        ))
        .build()
        .map_err(map_err)?;

    state
        .db
        .register_adapter_extended(adapter_params)
        .await
        .map_err(map_err)?;

    // Seed adapter stack
    sqlx::query(
        r#"
        INSERT INTO adapter_stacks (id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, determinism_mode, routing_determinism_mode, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, 'Parallel', '1.0.0', 'active', 'strict', 'deterministic', ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            adapter_ids_json = excluded.adapter_ids_json,
            updated_at = excluded.updated_at,
            determinism_mode = excluded.determinism_mode,
            routing_determinism_mode = excluded.routing_determinism_mode
        "#,
    )
    .bind(STACK_ID)
    .bind(TENANT_ID)
    .bind(STACK_NAME)
    .bind("Seed stack for Cypress chat flow")
    .bind(&pinned_primary)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .execute(pool)
    .await
    .map_err(map_err)?;

    sqlx::query(
        "UPDATE tenants SET default_stack_id = ?, default_pinned_adapter_ids = ? WHERE id = ?",
    )
    .bind(STACK_ID)
    .bind(&pinned_primary)
    .bind(TENANT_ID)
    .execute(pool)
    .await
    .map_err(map_err)?;

    // Seed secondary tenant + adapter to satisfy multi-tenant fixtures (default_stack_id=NULL initially)
    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, itar_flag, status, created_at, updated_at, default_stack_id, default_pinned_adapter_ids, determinism_mode)
        VALUES (?, ?, 0, 'active', ?, ?, NULL, ?, 'strict')
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            status = excluded.status,
            updated_at = excluded.updated_at,
            default_pinned_adapter_ids = excluded.default_pinned_adapter_ids
        "#,
    )
    .bind(TENANT_ID_SECONDARY)
    .bind(TENANT_NAME_SECONDARY)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .bind(&pinned_secondary)
    .execute(pool)
    .await
    .map_err(map_err)?;

    // Register secondary model
    sqlx::query("DELETE FROM models WHERE id = ?")
        .bind(MODEL_ID_SECONDARY)
        .execute(pool)
        .await
        .map_err(map_err)?;

    let model_params_secondary = ModelRegistrationBuilder::new()
        .name(MODEL_NAME_SECONDARY)
        .hash_b3("b3_model_qwen25_7b_test_secondary")
        .config_hash_b3("b3_model_config_test_secondary")
        .tokenizer_hash_b3("b3_model_tokenizer_test_secondary")
        .tokenizer_cfg_hash_b3("b3_model_tokenizer_cfg_test_secondary")
        .license_hash_b3(Some("b3_license_test_secondary"))
        .metadata_json(Some(
            serde_json::json!({"size_bytes": 1024_i64, "quant": "q4_0"}).to_string(),
        ))
        .build()
        .map_err(map_err)?;

    let inserted_model_id_secondary = state
        .db
        .register_model(model_params_secondary)
        .await
        .map_err(map_err)?;
    if inserted_model_id_secondary != MODEL_ID_SECONDARY {
        sqlx::query("UPDATE models SET id = ? WHERE id = ?")
            .bind(MODEL_ID_SECONDARY)
            .bind(&inserted_model_id_secondary)
            .execute(pool)
            .await
            .map_err(map_err)?;
    }

    sqlx::query(
        r#"
        UPDATE models SET
            tenant_id = ?,
            backend = 'mlx',
            quantization = 'q4_0',
            format = 'safetensors',
            size_bytes = 1024,
            import_status = 'available',
            imported_at = ?,
            imported_by = ?
        WHERE id = ?
        "#,
    )
    .bind(TENANT_ID_SECONDARY)
    .bind(FIXED_TS)
    .bind("seed-fixtures")
    .bind(MODEL_ID_SECONDARY)
    .execute(pool)
    .await
    .map_err(map_err)?;

    // Register secondary adapter tied to secondary model
    sqlx::query("DELETE FROM adapters WHERE adapter_id = ? AND tenant_id = ?")
        .bind(ADAPTER_ID_SECONDARY)
        .bind(TENANT_ID_SECONDARY)
        .execute(pool)
        .await
        .map_err(map_err)?;

    let adapter_params_secondary = AdapterRegistrationBuilder::new()
        .tenant_id(TENANT_ID_SECONDARY)
        .adapter_id(ADAPTER_ID_SECONDARY)
        .name(ADAPTER_NAME_SECONDARY)
        .hash_b3("b3_adapter_seed_test_secondary")
        .rank(8)
        .tier("warm")
        .alpha(16.0)
        .lora_strength(Some(1.0))
        .targets_json(r#"["attn.q_proj","attn.v_proj"]"#)
        .category("code")
        .scope("global")
        .base_model_id(Some(MODEL_ID_SECONDARY))
        .manifest_schema_version(Some("1.0.0"))
        .content_hash_b3(Some("b3_adapter_content_seed_secondary"))
        .metadata_json(Some(
            serde_json::json!({"description": "Secondary seed adapter for Cypress", "owner": "seed-fixtures"})
                .to_string(),
        ))
        .build()
        .map_err(map_err)?;

    state
        .db
        .register_adapter_extended(adapter_params_secondary)
        .await
        .map_err(map_err)?;

    // Seed secondary adapter stack
    sqlx::query(
        r#"
        INSERT INTO adapter_stacks (id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, determinism_mode, routing_determinism_mode, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, 'Parallel', '1.0.0', 'active', 'strict', 'deterministic', ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            adapter_ids_json = excluded.adapter_ids_json,
            updated_at = excluded.updated_at,
            determinism_mode = excluded.determinism_mode,
            routing_determinism_mode = excluded.routing_determinism_mode
        "#,
    )
    .bind(STACK_ID_SECONDARY)
    .bind(TENANT_ID_SECONDARY)
    .bind(STACK_NAME_SECONDARY)
    .bind("Secondary seed stack for Cypress chat flow")
    .bind(&pinned_secondary)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .execute(pool)
    .await
    .map_err(map_err)?;
    sqlx::query(
        "UPDATE tenants SET default_stack_id = ?, default_pinned_adapter_ids = ? WHERE id = ?",
    )
    .bind(STACK_ID_SECONDARY)
    .bind(&pinned_secondary)
    .bind(TENANT_ID_SECONDARY)
    .execute(pool)
    .await
    .map_err(map_err)?;

    // Create policy actor user for FK constraint satisfaction (granted_by references users.id)
    // Note: role must be one of: 'admin','operator','sre','compliance','auditor','viewer'
    sqlx::query(
        "INSERT OR IGNORE INTO users (id, email, display_name, pw_hash, role, tenant_id) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(POLICY_ACTOR)
    .bind("seed-fixtures@internal.local")
    .bind("Seed Fixtures Actor")
    .bind("internal-system")
    .bind("admin")  // Must be a valid role per CHECK constraint
    .bind(TENANT_ID)
    .execute(pool)
    .await
    .map_err(map_err)?;

    // Grant primary user access to secondary tenant for deterministic tenant switching in tests
    sqlx::query(
        r#"
        INSERT INTO user_tenant_access (id, user_id, tenant_id, granted_by, granted_at, reason, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, NULL)
        ON CONFLICT(user_id, tenant_id) DO UPDATE SET
            granted_by = excluded.granted_by,
            granted_at = excluded.granted_at,
            reason = excluded.reason,
            expires_at = excluded.expires_at
        "#,
    )
    .bind("user-tenant-access-fixture-secondary")
    .bind(E2E_USER_ID)
    .bind(TENANT_ID_SECONDARY)
    .bind(POLICY_ACTOR)
    .bind(FIXED_TS)
    .bind("seed-minimal")
    .execute(pool)
    .await
    .map_err(map_err)?;

    Ok(Json(SeedMinimalResponse {
        status: "ok".to_string(),
        tenant_id: TENANT_ID.to_string(),
        user_id: E2E_USER_ID.to_string(),
        model_id: MODEL_ID.to_string(),
        adapter_id: ADAPTER_ID.to_string(),
        stack_id: STACK_ID.to_string(),
        secondary_tenant_id: TENANT_ID_SECONDARY.to_string(),
        secondary_model_id: MODEL_ID_SECONDARY.to_string(),
        secondary_adapter_id: ADAPTER_ID_SECONDARY.to_string(),
        secondary_stack_id: STACK_ID_SECONDARY.to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct TraceFixtureRequest {
    pub tenant_id: Option<String>,
    pub token_count: Option<usize>,
    pub adapter_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct TraceFixtureResponse {
    pub trace_id: String,
    pub tenant_id: String,
    pub token_count: usize,
    pub adapter_ids: Vec<String>,
}

#[axum::debug_handler]
pub async fn create_trace_fixture(
    State(state): State<AppState>,
    Json(req): Json<TraceFixtureRequest>,
) -> Result<Json<TraceFixtureResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let tenant_id = req.tenant_id.unwrap_or_else(|| TENANT_ID.to_string());
    let adapter_ids = req
        .adapter_ids
        .filter(|ids| !ids.is_empty())
        .unwrap_or_else(|| vec![ADAPTER_ID.to_string(), ADAPTER_ID_SECONDARY.to_string()]);
    let token_count = req.token_count.unwrap_or(TRACE_TOKEN_COUNT).max(1);
    let trace_id = TRACE_ID.to_string();
    let pool = state.db.pool();

    // Clear existing fixture rows for idempotency
    sqlx::query("DELETE FROM inference_trace_receipts WHERE trace_id = ?")
        .bind(&trace_id)
        .execute(pool)
        .await
        .map_err(map_err)?;
    sqlx::query("DELETE FROM inference_trace_tokens WHERE trace_id = ?")
        .bind(&trace_id)
        .execute(pool)
        .await
        .map_err(map_err)?;
    sqlx::query("DELETE FROM inference_traces WHERE trace_id = ?")
        .bind(&trace_id)
        .execute(pool)
        .await
        .map_err(map_err)?;

    let context_digest = hash_bytes("trace-fixture-context");

    // Insert trace header
    sqlx::query(
        r#"
        INSERT INTO inference_traces (trace_id, tenant_id, request_id, context_digest, created_at, status)
        VALUES (?, ?, ?, ?, ?, 'completed')
        ON CONFLICT(trace_id) DO UPDATE SET
            tenant_id = excluded.tenant_id,
            request_id = excluded.request_id,
            context_digest = excluded.context_digest,
            created_at = excluded.created_at,
            status = excluded.status
        "#,
    )
    .bind(&trace_id)
    .bind(&tenant_id)
    .bind(TRACE_REQUEST_ID)
    .bind(context_digest.as_slice())
    .bind(FIXED_TS)
    .execute(pool)
    .await
    .map_err(map_err)?;

    let adapter_ids_json = serde_json::to_string(&adapter_ids).unwrap_or_else(|_| "[]".to_string());
    let gates: Vec<i16> = adapter_ids
        .iter()
        .enumerate()
        .map(|(idx, _)| 15000 + idx as i16 * 500)
        .collect();
    let gates_json = serde_json::to_string(&gates).unwrap_or_else(|_| "[]".to_string());

    for token_index in 0..token_count {
        let decision_hash =
            B3Hash::hash_multi(&[context_digest.as_slice(), &token_index.to_le_bytes()]);

        sqlx::query(
            r#"
            INSERT INTO inference_trace_tokens (
                trace_id, token_index, selected_adapter_ids, gates_q15, decision_hash,
                policy_mask_digest, backend_id, kernel_version_id, fusion_interval_id, fused_weight_hash, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?)
            "#,
        )
        .bind(&trace_id)
        .bind(token_index as i64)
        .bind(&adapter_ids_json)
        .bind(&gates_json)
        .bind(&decision_hash.as_bytes()[..])
        .bind::<Option<Vec<u8>>>(None)
        .bind(TRACE_BACKEND_ID)
        .bind(TRACE_KERNEL_VERSION)
        .bind(FIXED_TS)
        .execute(pool)
        .await
        .map_err(map_err)?;
    }

    // Insert deterministic receipt
    let run_head_hash = hash_bytes("trace-run-head");
    let output_digest = hash_bytes("trace-output");
    let receipt_digest = hash_bytes("trace-receipt");

    sqlx::query(
        r#"
        INSERT INTO inference_trace_receipts (trace_id, run_head_hash, output_digest, receipt_digest, created_at)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(trace_id) DO UPDATE SET
            run_head_hash = excluded.run_head_hash,
            output_digest = excluded.output_digest,
            receipt_digest = excluded.receipt_digest,
            created_at = excluded.created_at
        "#,
    )
    .bind(&trace_id)
    .bind(run_head_hash.as_slice())
    .bind(output_digest.as_slice())
    .bind(receipt_digest.as_slice())
    .bind(FIXED_TS)
    .execute(pool)
    .await
    .map_err(map_err)?;

    Ok(Json(TraceFixtureResponse {
        trace_id,
        tenant_id,
        token_count,
        adapter_ids,
    }))
}

#[derive(Debug, Deserialize)]
pub struct DiagRunFixtureRequest {
    pub run_id: Option<String>,
    pub trace_id: Option<String>,
    pub tenant_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagRunFixtureResponse {
    pub run_id: String,
    pub trace_id: String,
    pub tenant_id: String,
}

#[axum::debug_handler]
pub async fn create_diag_run_fixture(
    State(state): State<AppState>,
    Json(req): Json<DiagRunFixtureRequest>,
) -> Result<Json<DiagRunFixtureResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let tenant_id = req.tenant_id.unwrap_or_else(|| TENANT_ID.to_string());
    let run_id = req.run_id.unwrap_or_else(|| TRACE_ID.to_string());
    let trace_id = req.trace_id.unwrap_or_else(|| run_id.clone());
    let status = req.status.unwrap_or_else(|| "completed".to_string());
    let request_hash = "b3:diag-run-request-fixture";

    sqlx::query("DELETE FROM diag_runs WHERE id = ?")
        .bind(&run_id)
        .execute(state.db.pool())
        .await
        .map_err(map_err)?;

    sqlx::query(
        r#"
        INSERT INTO diag_runs (
            id, tenant_id, trace_id, started_at_unix_ms, completed_at_unix_ms,
            request_hash, manifest_hash, status, total_events_count
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&run_id)
    .bind(&tenant_id)
    .bind(&trace_id)
    .bind(FIXED_TS_MS)
    .bind(FIXED_TS_MS + 1000)
    .bind(request_hash)
    .bind("b3:diag-run-manifest-fixture")
    .bind(&status)
    .bind(1_i64)
    .execute(state.db.pool())
    .await
    .map_err(map_err)?;

    Ok(Json(DiagRunFixtureResponse {
        run_id,
        trace_id,
        tenant_id,
    }))
}

#[derive(Debug, Deserialize)]
pub struct EvidenceFixtureRequest {
    pub tenant_id: Option<String>,
    pub inference_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EvidenceFixtureResponse {
    pub evidence_id: String,
    pub tenant_id: String,
    pub inference_id: String,
    pub document_id: String,
    pub chunk_id: String,
}

#[axum::debug_handler]
pub async fn create_evidence_fixture(
    State(state): State<AppState>,
    Json(req): Json<EvidenceFixtureRequest>,
) -> Result<Json<EvidenceFixtureResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let tenant_id = req.tenant_id.unwrap_or_else(|| TENANT_ID.to_string());
    let inference_id = req.inference_id.unwrap_or_else(|| TRACE_ID.to_string());

    let pool = state.db.pool();

    // Clear old fixture rows
    sqlx::query("DELETE FROM inference_evidence WHERE id = ?")
        .bind(EVIDENCE_ID)
        .execute(pool)
        .await
        .map_err(map_err)?;
    sqlx::query("DELETE FROM document_chunks WHERE id = ?")
        .bind(DOCUMENT_CHUNK_ID)
        .execute(pool)
        .await
        .map_err(map_err)?;
    sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(DOCUMENT_ID)
        .execute(pool)
        .await
        .map_err(map_err)?;

    // Document + chunk fixtures
    sqlx::query(
        r#"
        INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, page_count, status, created_at, updated_at, metadata_json)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'ready', ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            tenant_id = excluded.tenant_id,
            name = excluded.name,
            content_hash = excluded.content_hash,
            file_path = excluded.file_path,
            file_size = excluded.file_size,
            mime_type = excluded.mime_type,
            page_count = excluded.page_count,
            status = excluded.status,
            updated_at = excluded.updated_at,
            metadata_json = excluded.metadata_json
        "#,
    )
    .bind(DOCUMENT_ID)
    .bind(&tenant_id)
    .bind("Fixture Document")
    .bind("b3_doc_fixture_hash")
    .bind("var/testkit/doc-fixture.txt")
    .bind(1024_i64)
    .bind("text/plain")
    .bind(1_i64)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .bind(r#"{"source":"testkit"}"#)
    .execute(pool)
    .await
    .map_err(map_err)?;

    sqlx::query(
        r#"
        INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, page_number, start_offset, end_offset, chunk_hash, text_preview, embedding_json)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            tenant_id = excluded.tenant_id,
            document_id = excluded.document_id,
            chunk_index = excluded.chunk_index,
            page_number = excluded.page_number,
            start_offset = excluded.start_offset,
            end_offset = excluded.end_offset,
            chunk_hash = excluded.chunk_hash,
            text_preview = excluded.text_preview,
            embedding_json = excluded.embedding_json
        "#,
    )
    .bind(DOCUMENT_CHUNK_ID)
    .bind(&tenant_id)
    .bind(DOCUMENT_ID)
    .bind(0_i64)
    .bind(1_i64)
    .bind(0_i64)
    .bind(50_i64)
    .bind("b3_chunk_fixture_hash")
    .bind("Deterministic chunk for Cypress evidence")
    .bind(r#"{"vector":[0.1,0.2,0.3]}"#)
    .execute(pool)
    .await
    .map_err(map_err)?;

    let context_hash = B3Hash::hash(b"fixture-context").to_hex();
    let rag_doc_ids =
        serde_json::to_string(&vec![DOCUMENT_ID]).unwrap_or_else(|_| "[]".to_string());
    let rag_scores = serde_json::to_string(&vec![0.98_f64]).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        r#"
        INSERT INTO inference_evidence (
            id, tenant_id, inference_id, session_id, message_id, document_id, chunk_id,
            page_number, document_hash, chunk_hash, relevance_score, rank,
            context_hash, created_at, rag_doc_ids, rag_scores, rag_collection_id
        )
        VALUES (?, ?, ?, NULL, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)
        ON CONFLICT(id) DO UPDATE SET
            tenant_id = excluded.tenant_id,
            inference_id = excluded.inference_id,
            document_id = excluded.document_id,
            chunk_id = excluded.chunk_id,
            page_number = excluded.page_number,
            document_hash = excluded.document_hash,
            chunk_hash = excluded.chunk_hash,
            relevance_score = excluded.relevance_score,
            rank = excluded.rank,
            context_hash = excluded.context_hash,
            rag_doc_ids = excluded.rag_doc_ids,
            rag_scores = excluded.rag_scores,
            rag_collection_id = excluded.rag_collection_id
        "#,
    )
    .bind(EVIDENCE_ID)
    .bind(&tenant_id)
    .bind(&inference_id)
    .bind(DOCUMENT_ID)
    .bind(DOCUMENT_CHUNK_ID)
    .bind(1_i64)
    .bind("b3_doc_fixture_hash")
    .bind("b3_chunk_fixture_hash")
    .bind(0.98_f64)
    .bind(1_i64)
    .bind(&context_hash)
    .bind(FIXED_TS)
    .bind(&rag_doc_ids)
    .bind(&rag_scores)
    .execute(pool)
    .await
    .map_err(map_err)?;

    Ok(Json(EvidenceFixtureResponse {
        evidence_id: EVIDENCE_ID.to_string(),
        tenant_id,
        inference_id,
        document_id: DOCUMENT_ID.to_string(),
        chunk_id: DOCUMENT_CHUNK_ID.to_string(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateRepoRequest {
    pub repo_id: Option<String>,
    pub tenant_id: Option<String>,
    pub name: Option<String>,
    pub base_model_id: Option<String>,
    pub default_branch: Option<String>,
    pub path: Option<String>,
    pub languages: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct CreateRepoResponse {
    pub repo_id: String,
}

#[axum::debug_handler]
pub async fn create_repo(
    State(state): State<AppState>,
    Json(req): Json<CreateRepoRequest>,
) -> Result<Json<CreateRepoResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let tenant_id = req.tenant_id.unwrap_or_else(|| TENANT_ID.to_string());
    let repo_id = req.repo_id.unwrap_or_else(|| "repo-e2e".to_string());
    let name = req.name.unwrap_or_else(|| "e2e-repo".to_string());
    let default_branch = req.default_branch.as_deref().unwrap_or("main");
    let repo_path = req.path.unwrap_or_else(|| format!("var/repos/{}", repo_id));
    let languages = req.languages.unwrap_or_else(|| vec!["Rust".to_string()]);

    let created_id = state
        .db
        .create_adapter_repository(CreateAdapterRepositoryParams {
            tenant_id: &tenant_id,
            name: &name,
            base_model_id: req.base_model_id.as_deref(),
            default_branch: Some(default_branch),
            created_by: Some(POLICY_ACTOR),
            description: Some("E2E test repository"),
        })
        .await
        .map_err(map_err)?;

    if created_id != repo_id {
        sqlx::query("UPDATE adapter_repositories SET id = ? WHERE id = ?")
            .bind(&repo_id)
            .bind(&created_id)
            .execute(state.db.pool())
            .await
            .map_err(map_err)?;
    }

    // Ensure code repository entry exists for /v1/code/repositories list.
    sqlx::query("DELETE FROM repositories WHERE tenant_id = ? AND repo_id = ?")
        .bind(&tenant_id)
        .bind(&repo_id)
        .execute(state.db.pool())
        .await
        .map_err(map_err)?;

    state
        .db
        .register_repository(&tenant_id, &repo_id, &repo_path, &languages, default_branch)
        .await
        .map_err(map_err)?;

    // Status defaults to "registered" for newly inserted repositories.

    Ok(Json(CreateRepoResponse { repo_id }))
}

#[derive(Debug, Deserialize)]
pub struct CreateAdapterVersionRequest {
    pub version_id: Option<String>,
    pub repo_id: String,
    pub tenant_id: Option<String>,
    pub version: Option<String>,
    pub branch: Option<String>,
    pub branch_classification: Option<String>,
    pub aos_hash: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateAdapterVersionResponse {
    pub version_id: String,
}

#[axum::debug_handler]
pub async fn create_adapter_version(
    State(state): State<AppState>,
    Json(req): Json<CreateAdapterVersionRequest>,
) -> Result<Json<CreateAdapterVersionResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let tenant_id = req.tenant_id.unwrap_or_else(|| TENANT_ID.to_string());
    let version_id = req
        .version_id
        .unwrap_or_else(|| "adapter-version-e2e".to_string());
    let version = req.version.unwrap_or_else(|| "1.0.0".to_string());
    let branch = req.branch.unwrap_or_else(|| "main".to_string());
    let branch_classification = req
        .branch_classification
        .unwrap_or_else(|| "protected".to_string());

    let created = state
        .db
        .create_adapter_version(CreateAdapterVersionParams {
            repo_id: &req.repo_id,
            tenant_id: &tenant_id,
            version: &version,
            branch: &branch,
            branch_classification: &branch_classification,
            aos_path: None,
            aos_hash: req.aos_hash.as_deref(),
            manifest_schema_version: Some("1.0.0"),
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: None,
            training_backend: None,
            coreml_used: Some(false),
            coreml_device_type: None,
            dataset_version_ids: None,
            release_state: "ready",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: true,
            actor: Some(POLICY_ACTOR),
            reason: Some("testkit-create-version"),
            train_job_id: None,
        })
        .await
        .map_err(map_err)?;

    if created != version_id {
        sqlx::query("UPDATE adapter_versions SET id = ? WHERE id = ?")
            .bind(&version_id)
            .bind(&created)
            .execute(state.db.pool())
            .await
            .map_err(map_err)?;
    }

    Ok(Json(CreateAdapterVersionResponse { version_id }))
}

#[derive(Debug, Deserialize)]
pub struct SetPolicyRequest {
    pub tenant_id: Option<String>,
    pub policy_id: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct SetPolicyResponse {
    pub previous_enabled: bool,
    pub new_enabled: bool,
}

#[axum::debug_handler]
pub async fn set_policy(
    State(state): State<AppState>,
    Json(req): Json<SetPolicyRequest>,
) -> Result<Json<SetPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let tenant_id = req.tenant_id.unwrap_or_else(|| TENANT_ID.to_string());

    let previous = state
        .db
        .toggle_tenant_policy(&tenant_id, &req.policy_id, req.enabled, POLICY_ACTOR)
        .await
        .map_err(map_err)?;

    Ok(Json(SetPolicyResponse {
        previous_enabled: previous,
        new_enabled: req.enabled,
    }))
}

#[derive(Debug, Deserialize)]
pub struct TrainingJobStubRequest {
    pub job_id: Option<String>,
    pub repo_id: Option<String>,
    pub status: Option<String>,
    pub tenant_id: Option<String>,
    pub adapter_id: Option<String>,
    pub adapter_name: Option<String>,
    pub base_model_id: Option<String>,
    pub stack_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TrainingJobStubResponse {
    pub job_id: String,
}

#[axum::debug_handler]
pub async fn create_training_job_stub(
    State(state): State<AppState>,
    Json(req): Json<TrainingJobStubRequest>,
) -> Result<Json<TrainingJobStubResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let job_id = req.job_id.unwrap_or_else(|| "job-stub".to_string());
    let repo_id = req.repo_id.unwrap_or_else(|| "repo-e2e".to_string());
    let status = req.status.unwrap_or_else(|| "completed".to_string());
    let tenant_id = req.tenant_id.unwrap_or_else(|| TENANT_ID.to_string());
    let adapter_id = req.adapter_id.unwrap_or_else(|| ADAPTER_ID.to_string());
    let adapter_name = req.adapter_name.unwrap_or_else(|| ADAPTER_NAME.to_string());
    let base_model_id = req.base_model_id.unwrap_or_else(|| MODEL_ID.to_string());
    let stack_id = req.stack_id.unwrap_or_else(|| STACK_ID.to_string());
    let progress_json = serde_json::json!({
        "progress_pct": if status == "completed" { 100.0 } else { 0.0 },
        "current_epoch": 1,
        "total_epochs": 1,
        "current_loss": 0.0,
        "learning_rate": 0.0005,
        "tokens_per_second": 10.0,
        "error_message": null
    })
    .to_string();

    // Only reference models that belong to the same tenant, otherwise adapters inserts
    // will fail tenant-isolation triggers.
    let base_model_id_fk = match sqlx::query(
        "SELECT 1 FROM models WHERE id = ? AND tenant_id = ? LIMIT 1",
    )
    .bind(&base_model_id)
    .bind(&tenant_id)
    .fetch_optional(state.db.pool())
    .await
    {
        Ok(Some(_)) => Some(base_model_id.as_str()),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to check base model existence for training job stub; continuing without base_model_id");
            None
        }
    };

    // Same tenant guard as models: avoid referencing stacks from another tenant.
    let stack_id_fk = match sqlx::query(
        "SELECT 1 FROM adapter_stacks WHERE id = ? AND tenant_id = ? LIMIT 1",
    )
    .bind(&stack_id)
    .bind(&tenant_id)
    .fetch_optional(state.db.pool())
    .await
    {
        Ok(Some(_)) => Some(stack_id.as_str()),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to check stack existence for training job stub; continuing without stack_id");
            None
        }
    };

    // Make sure the adapter referenced by the stub job exists, so UI handoff actions
    // ("View Adapter" / "Try in Chat") don't 404 during demo flows.
    //
    // IMPORTANT: `repository_training_jobs.adapter_id` is matched against `adapters.id` by triggers,
    // so for demo determinism we set `adapters.id == adapter_id` and also populate `adapters.adapter_id`
    // for external lookups.
    //
    // Best-effort: if this fails we still want the job row for troubleshooting.
    let adapter_hash_b3 = format!(
        "b3_testkit_{}",
        B3Hash::hash(format!("testkit-adapter:{tenant_id}:{adapter_id}").as_bytes()).to_hex()
    );
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO adapters (
            id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json,
            adapter_id, category, scope, base_model_id, manifest_schema_version, content_hash_b3,
            metadata_json, updated_at
        )
        VALUES (
            ?, ?, ?, 'warm', ?, 8, 16.0, ?,
            ?, 'code', 'global', ?, '1.0.0', ?,
            ?, datetime('now')
        )
        ON CONFLICT(id) DO UPDATE SET
            tenant_id = excluded.tenant_id,
            name = excluded.name,
            tier = excluded.tier,
            hash_b3 = excluded.hash_b3,
            rank = excluded.rank,
            alpha = excluded.alpha,
            targets_json = excluded.targets_json,
            adapter_id = excluded.adapter_id,
            category = excluded.category,
            scope = excluded.scope,
            base_model_id = excluded.base_model_id,
            manifest_schema_version = excluded.manifest_schema_version,
            content_hash_b3 = excluded.content_hash_b3,
            metadata_json = excluded.metadata_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&adapter_id) // id
    .bind(&tenant_id)
    .bind(&adapter_name)
    .bind(&adapter_hash_b3)
    .bind(r#"["attn.q_proj","attn.v_proj"]"#)
    .bind(&adapter_id) // external adapter_id
    .bind(base_model_id_fk)
    .bind("b3_testkit_adapter_content")
    .bind(serde_json::json!({"description": "Testkit adapter for training stub demo"}).to_string())
    .execute(state.db.pool())
    .await
    {
        tracing::warn!(error = %e, "Failed to seed adapter for training job stub");
    }

    sqlx::query(
        r#"
        INSERT INTO git_repositories (
            id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, 'ready', ?)
        ON CONFLICT(repo_id) DO UPDATE SET
            path = excluded.path,
            branch = excluded.branch,
            analysis_json = excluded.analysis_json,
            evidence_json = excluded.evidence_json,
            security_scan_json = excluded.security_scan_json,
            status = excluded.status,
            created_by = excluded.created_by
        "#,
    )
    .bind(format!(
        "git-{}",
        &B3Hash::hash(format!("testkit-git-repo:{repo_id}").as_bytes()).to_hex()[..16]
    ))
    .bind(&repo_id)
    // Keep runtime/test fixtures under var/testkit (var/tmp is discouraged and often cleaned).
    .bind("var/testkit/repo-e2e")
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind(POLICY_ACTOR)
    .execute(state.db.pool())
    .await
    .map_err(map_err)?;

    sqlx::query("DELETE FROM repository_training_jobs WHERE id = ?")
        .bind(&job_id)
        .execute(state.db.pool())
        .await
        .map_err(map_err)?;

    sqlx::query(
        r#"
        INSERT INTO repository_training_jobs (
            id, repo_id, tenant_id, training_config_json, status, progress_json, created_by,
            adapter_name, adapter_id, base_model_id, stack_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&job_id)
    .bind(&repo_id)
    .bind(&tenant_id)
    .bind(r#"{"rank":8,"alpha":16,"epochs":1,"learning_rate":0.0005,"batch_size":4}"#)
    .bind(&status)
    .bind(&progress_json)
    .bind(POLICY_ACTOR)
    .bind(&adapter_name)
    .bind(&adapter_id)
    .bind(base_model_id_fk)
    .bind(stack_id_fk)
    .execute(state.db.pool())
    .await
    .map_err(map_err)?;

    Ok(Json(TrainingJobStubResponse { job_id }))
}

#[derive(Debug, Deserialize)]
pub struct DocumentFixtureRequest {
    pub document_id: Option<String>,
    pub tenant_id: Option<String>,
    pub status: Option<String>, // "failed" | "pending" | "processing" | "indexed" | "ready"
    pub name: Option<String>,
    pub mime_type: Option<String>, // default "application/pdf"
    pub error_message: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DocumentFixtureResponse {
    pub document_id: String,
    pub tenant_id: String,
    pub status: String,
}

#[axum::debug_handler]
pub async fn create_document_fixture(
    State(state): State<AppState>,
    Json(req): Json<DocumentFixtureRequest>,
) -> Result<Json<DocumentFixtureResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;

    let tenant_id = req.tenant_id.unwrap_or_else(|| TENANT_ID.to_string());
    let document_id = req
        .document_id
        .unwrap_or_else(|| "doc-fixture-demo".to_string());
    let status = req.status.unwrap_or_else(|| "failed".to_string());
    let name = req
        .name
        .unwrap_or_else(|| "Demo Fixture Document".to_string());
    let mime_type = req
        .mime_type
        .unwrap_or_else(|| "application/pdf".to_string());

    // The documents table enforces (tenant_id, content_hash) uniqueness.
    // For fixtures we want stable-but-distinct hashes per (tenant_id, document_id).
    let content_hash =
        B3Hash::hash(format!("testkit-doc:{tenant_id}:{document_id}").as_bytes()).to_hex();

    // Create a small on-disk fixture file so download/processing flows have a real path.
    let dir = std::path::Path::new("var/testkit");
    if let Err(e) = tokio::fs::create_dir_all(dir).await {
        return Err(map_err(e));
    }
    let file_path = format!("var/testkit/{}.pdf", document_id);
    // Prefer a real fixture PDF with a text layer (so ingest succeeds), but fall back to a minimal
    // valid PDF if the repo fixture is unavailable.
    //
    // NOTE: The minimal PDF is byte-for-byte identical to the ingest-docs integration test fixture.
    const FALLBACK_PDF_BYTES: &[u8] = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>\nendobj\n4 0 obj\n<< /Length 0 >>\nstream\n\nendstream\nendobj\nxref\n0 5\n0000000000 65535 f \n0000000009 00000 n \n0000000058 00000 n \n0000000115 00000 n \n0000000202 00000 n \ntrailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n251\n%%EOF\n";
    let pdf_bytes = match tokio::fs::read("tests/fixtures/docs/training_overview.pdf").await {
        Ok(bytes) => bytes,
        Err(_) => FALLBACK_PDF_BYTES.to_vec(),
    };
    if let Err(e) = tokio::fs::write(&file_path, &pdf_bytes).await {
        return Err(map_err(e));
    }

    // Ensure row exists in SQL documents table for list/detail views.
    sqlx::query(
        r#"
        INSERT INTO documents (
            id, tenant_id, name, content_hash, file_path, file_size, mime_type, page_count,
            status, created_at, updated_at, metadata_json,
            error_message, error_code, retry_count, max_retries, processing_started_at, processing_completed_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 3, NULL, NULL)
        ON CONFLICT(id) DO UPDATE SET
            tenant_id = excluded.tenant_id,
            name = excluded.name,
            content_hash = excluded.content_hash,
            file_path = excluded.file_path,
            file_size = excluded.file_size,
            mime_type = excluded.mime_type,
            status = excluded.status,
            updated_at = excluded.updated_at,
            error_message = excluded.error_message,
            error_code = excluded.error_code,
            retry_count = 0,
            max_retries = excluded.max_retries,
            processing_started_at = NULL,
            processing_completed_at = NULL
        "#,
    )
    .bind(&document_id)
    .bind(&tenant_id)
    .bind(&name)
    .bind(&content_hash)
    .bind(&file_path)
    .bind(pdf_bytes.len() as i64)
    .bind(&mime_type)
    .bind(1_i64)
    .bind(&status)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .bind(r#"{"source":"testkit","fixture":"document"}"#)
    .bind(req.error_message.as_deref())
    .bind(req.error_code.as_deref())
    .execute(state.db.pool())
    .await
    .map_err(map_err)?;

    // Re-creating a fixture should be idempotent: clear any existing chunks so a subsequent
    // reprocess doesn't trip UNIQUE(document_id, chunk_index).
    sqlx::query("DELETE FROM document_chunks WHERE tenant_id = ? AND document_id = ?")
        .bind(&tenant_id)
        .bind(&document_id)
        .execute(state.db.pool())
        .await
        .map_err(map_err)?;

    Ok(Json(DocumentFixtureResponse {
        document_id,
        tenant_id,
        status,
    }))
}

#[derive(Debug, Deserialize)]
pub struct InferenceStubRequest {
    pub prompt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InferenceStubResponse {
    pub schema_version: String,
    pub id: String,
    pub text: String,
    pub tokens_generated: u32,
    pub token_count: u32,
    pub latency_ms: u32,
    pub adapters_used: Vec<String>,
    pub finish_reason: String,
    pub backend: String,
    pub backend_used: String,
    pub run_receipt: RunReceipt,
    pub trace: StubTrace,
}

#[derive(Debug, Serialize)]
pub struct RunReceipt {
    pub trace_id: String,
    pub run_head_hash: String,
    pub output_digest: String,
    pub receipt_digest: String,
    pub logical_prompt_tokens: u32,
    pub prefix_cached_token_count: u32,
    pub billed_input_tokens: u32,
    pub logical_output_tokens: u32,
    pub billed_output_tokens: u32,
    #[serde(default)]
    pub stop_reason_code: Option<String>,
    #[serde(default)]
    pub stop_reason_token_index: Option<u32>,
    #[serde(default)]
    pub stop_policy_digest_b3: Option<String>,
    #[serde(default)]
    pub tenant_kv_quota_bytes: u64,
    #[serde(default)]
    pub tenant_kv_bytes_used: u64,
    #[serde(default)]
    pub kv_evictions: u32,
    #[serde(default)]
    pub kv_residency_policy_id: Option<String>,
    #[serde(default)]
    pub kv_quota_enforced: bool,
    #[serde(default)]
    pub prefix_kv_key_b3: Option<String>,
    #[serde(default)]
    pub prefix_cache_hit: bool,
    #[serde(default)]
    pub prefix_kv_bytes: u64,
    #[serde(default)]
    pub model_cache_identity_v2_digest_b3: Option<String>,
    #[serde(default)]
    pub previous_receipt_digest: Option<String>,
    #[serde(default)]
    pub session_sequence: u64,
    #[serde(default)]
    pub tokenizer_hash_b3: Option<String>,
    #[serde(default)]
    pub tokenizer_version: Option<String>,
    #[serde(default)]
    pub tokenizer_normalization: Option<String>,
    #[serde(default)]
    pub model_build_hash_b3: Option<String>,
    #[serde(default)]
    pub adapter_build_hash_b3: Option<String>,
    #[serde(default)]
    pub decode_algo: Option<String>,
    #[serde(default)]
    pub temperature_q15: Option<i16>,
    #[serde(default)]
    pub top_p_q15: Option<i16>,
    #[serde(default)]
    pub top_k: Option<u32>,
    #[serde(default)]
    pub seed_digest_b3: Option<String>,
    #[serde(default)]
    pub sampling_backend: Option<String>,
    #[serde(default)]
    pub thread_count: Option<u32>,
    #[serde(default)]
    pub reduction_strategy: Option<String>,
    #[serde(default)]
    pub stop_eos_q15: Option<i16>,
    #[serde(default)]
    pub stop_window_digest_b3: Option<String>,
    #[serde(default)]
    pub cache_scope: Option<String>,
    #[serde(default)]
    pub cached_prefix_digest_b3: Option<String>,
    #[serde(default)]
    pub cached_prefix_len: Option<u32>,
    #[serde(default)]
    pub cache_key_b3: Option<String>,
    #[serde(default)]
    pub retrieval_merkle_root_b3: Option<String>,
    #[serde(default)]
    pub retrieval_order_digest_b3: Option<String>,
    #[serde(default)]
    pub tool_call_inputs_digest_b3: Option<String>,
    #[serde(default)]
    pub tool_call_outputs_digest_b3: Option<String>,
    #[serde(default)]
    pub disclosure_level: Option<String>,
    #[serde(default)]
    pub receipt_signing_kid: Option<String>,
    #[serde(default)]
    pub receipt_signed_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StubTrace {
    pub latency_ms: u32,
    pub adapters_used: Vec<String>,
    pub router_decisions: Vec<serde_json::Value>,
    pub evidence_spans: Vec<serde_json::Value>,
}

#[axum::debug_handler]
pub async fn inference_stub(
    Json(req): Json<InferenceStubRequest>,
) -> Result<Json<InferenceStubResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let text = req
        .prompt
        .map(|p| format!("Echo: {}", p))
        .unwrap_or_else(|| "Mocked inference output text".to_string());

    Ok(Json(InferenceStubResponse {
        schema_version: "1.0".to_string(),
        id: "resp-e2e".to_string(),
        text,
        tokens_generated: 8,
        token_count: 8,
        latency_ms: 42,
        adapters_used: vec![ADAPTER_ID.to_string()],
        finish_reason: "stop".to_string(),
        backend: "coreml".to_string(),
        backend_used: "coreml".to_string(),
        run_receipt: RunReceipt {
            trace_id: "trace-e2e".to_string(),
            run_head_hash: "head-hash-e2e".to_string(),
            output_digest: "output-digest-e2e".to_string(),
            receipt_digest: "b3-e2e-receipt-digest".to_string(),
            logical_prompt_tokens: 16,
            prefix_cached_token_count: 0,
            billed_input_tokens: 16,
            logical_output_tokens: 8,
            billed_output_tokens: 8,
            stop_reason_code: Some("stop".to_string()),
            stop_reason_token_index: Some(7),
            stop_policy_digest_b3: Some("stop-policy-digest-b3".to_string()),
            tenant_kv_quota_bytes: 0,
            tenant_kv_bytes_used: 0,
            kv_evictions: 0,
            kv_residency_policy_id: None,
            kv_quota_enforced: false,
            prefix_kv_key_b3: None,
            prefix_cache_hit: false,
            prefix_kv_bytes: 0,
            model_cache_identity_v2_digest_b3: None,
            previous_receipt_digest: None,
            session_sequence: 0,
            tokenizer_hash_b3: None,
            tokenizer_version: None,
            tokenizer_normalization: None,
            model_build_hash_b3: None,
            adapter_build_hash_b3: None,
            decode_algo: Some("greedy".to_string()),
            temperature_q15: None,
            top_p_q15: None,
            top_k: None,
            seed_digest_b3: None,
            sampling_backend: Some("coreml".to_string()),
            thread_count: None,
            reduction_strategy: None,
            stop_eos_q15: None,
            stop_window_digest_b3: None,
            cache_scope: Some("global".to_string()),
            cached_prefix_digest_b3: None,
            cached_prefix_len: Some(0),
            cache_key_b3: None,
            retrieval_merkle_root_b3: None,
            retrieval_order_digest_b3: None,
            tool_call_inputs_digest_b3: None,
            tool_call_outputs_digest_b3: None,
            disclosure_level: Some("full".to_string()),
            receipt_signing_kid: None,
            receipt_signed_at: None,
        },
        trace: StubTrace {
            latency_ms: 42,
            adapters_used: vec![ADAPTER_ID.to_string()],
            router_decisions: vec![],
            evidence_spans: vec![],
        },
    }))
}

#[derive(Debug, Deserialize)]
pub struct DivergeAuditQuery {
    pub tenant_id: Option<String>,
}

#[axum::debug_handler]
pub async fn diverge_policy_audit_chain(
    State(state): State<AppState>,
    claims: Option<Extension<Claims>>,
    Query(params): Query<DivergeAuditQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    ensure_e2e_mode()?;
    let Some(Extension(claims)) = claims else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("authentication required")
                    .with_code("UNAUTHORIZED")
                    .with_string_details("Provide claims to diverge audit chain"),
            ),
        ));
    };

    let tenant_id = params.tenant_id.unwrap_or_else(|| claims.tenant_id.clone());

    validate_tenant_isolation(&claims, &tenant_id)?;

    let (entry_id, entry_hash, chain_sequence) = state
        .db
        .force_corrupt_policy_audit_tail(&tenant_id)
        .await
        .map_err(|e| {
            if adapteros_db::policy_audit::is_audit_chain_divergence(&e) {
                return (
                    StatusCode::CONFLICT,
                    Json(
                        ErrorResponse::new("policy audit chain diverged")
                            .with_code(AUDIT_CHAIN_DIVERGED_CODE)
                            .with_string_details(e.to_string()),
                    ),
                );
            }
            map_err(e)
        })?;

    Ok(Json(json!({
        "status": "ok",
        "tenant_id": tenant_id,
        "corrupted_entry_id": entry_id,
        "corrupted_hash": entry_hash,
        "chain_sequence": chain_sequence,
    })))
}

pub fn register_routes() -> axum::Router<AppState> {
    use axum::routing::post;

    axum::Router::new()
        .route("/testkit/reset", post(reset))
        .route("/testkit/seed_minimal", post(seed_minimal))
        .route("/testkit/create_trace_fixture", post(create_trace_fixture))
        .route(
            "/testkit/create_diag_run_fixture",
            post(create_diag_run_fixture),
        )
        .route(
            "/testkit/create_evidence_fixture",
            post(create_evidence_fixture),
        )
        .route("/testkit/create_repo", post(create_repo))
        .route(
            "/testkit/create_adapter_version",
            post(create_adapter_version),
        )
        .route("/testkit/set_policy", post(set_policy))
        .route("/testkit/set_policy_fixture", post(set_policy))
        .route(
            "/testkit/create_training_job_stub",
            post(create_training_job_stub),
        )
        .route(
            "/testkit/create_document_fixture",
            post(create_document_fixture),
        )
        .route("/testkit/inference_stub", post(inference_stub))
        .route("/testkit/audit/diverge", post(diverge_policy_audit_chain))
}

pub fn e2e_enabled() -> bool {
    e2e_env_enabled()
}
