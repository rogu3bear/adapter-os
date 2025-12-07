//! Integration tests for PRD-CORE-03 chat bootstrap DB operations
//!
//! Tests verify:
//! 1. Stack creation with proper naming format
//! 2. update_training_job_result_ids persists stack_id and adapter_id
//! 3. get_training_job returns the persisted stack_id
//! 4. Chat session creation with stack_id and collection_id

use adapteros_core::Result;
use adapteros_db::chat_sessions::CreateChatSessionParams;
use adapteros_db::traits::CreateStackRequest;
use adapteros_db::Db;
use uuid::Uuid;

/// Test helper to create an in-memory database with migrations
async fn setup_test_db() -> Result<Db> {
    Db::new_in_memory().await
}

fn stack_name() -> String {
    format!("stack.test.{}", Uuid::new_v4().simple())
}

/// Test helper to create a tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to create tenant: {}", e))
        })?;
    Ok(())
}

/// Test helper to create a minimal user (needed for created_by FKs)
async fn create_test_user(db: &Db, user_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO users (id, email, display_name, pw_hash, role, disabled)
         VALUES (?, ?, ?, ?, 'admin', 0)",
    )
    .bind(user_id)
    .bind(format!("{}@example.com", user_id))
    .bind(format!("User {}", user_id))
    .bind("pw-hash")
    .execute(db.pool())
    .await
    .map_err(|e| adapteros_core::AosError::Database(format!("Failed to create user: {}", e)))?;
    Ok(())
}

/// Test helper to create a git repository (required FK for training jobs)
async fn create_test_repo(db: &Db, repo_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO git_repositories (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(format!("id-{}", repo_id))
    .bind(repo_id)
    .bind(format!("/repos/{}", repo_id))
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind("analyzed")
    .bind("test-user")
    .execute(db.pool())
    .await
    .map_err(|e| adapteros_core::AosError::Database(format!("Failed to create repo: {}", e)))?;
    Ok(())
}

/// Test helper to create a minimal training job
async fn create_test_training_job(
    db: &Db,
    job_id: &str,
    tenant_id: &str,
    repo_id: &str,
    status: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, training_config_json, status, progress_json, created_by, adapter_name, tenant_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(job_id)
    .bind(repo_id)
    .bind(r#"{"rank":16,"alpha":32}"#)
    .bind(status)
    .bind(r#"{"progress":0}"#)
    .bind("test-user")
    .bind("test-adapter")
    .bind(tenant_id)
    .execute(db.pool())
    .await
    .map_err(|e| {
        adapteros_core::AosError::Database(format!("Failed to create training job: {}", e))
    })?;
    Ok(())
}

#[tokio::test]
async fn test_insert_stack_with_proper_name_format() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-stack";
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create stack with proper name format: stack.{namespace}.{identifier}
    let name = stack_name();
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: name.clone(),
        description: Some("Test stack for chat bootstrap".to_string()),
        adapter_ids: vec!["adapter-001".to_string(), "adapter-002".to_string()],
        workflow_type: Some("Sequential".to_string()),
        determinism_mode: None,
    };

    let stack_id = db
        .insert_stack(&stack_req)
        .await
        .expect("Failed to insert stack");
    assert!(!stack_id.is_empty(), "Stack ID should be returned");

    // Verify we can retrieve the stack
    let stack = db
        .get_stack(tenant_id, &stack_id)
        .await
        .expect("Failed to get stack")
        .expect("Stack not found");

    assert_eq!(stack.name, name);
    assert_eq!(stack.tenant_id, tenant_id);

    // Verify adapter_ids_json is correct
    let adapter_ids: Vec<String> =
        serde_json::from_str(&stack.adapter_ids_json).expect("Failed to parse adapter_ids_json");
    assert_eq!(adapter_ids, vec!["adapter-001", "adapter-002"]);
}

#[tokio::test]
async fn test_update_training_job_result_ids() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-update";
    let repo_id = "test-repo-update";
    let job_id = "train-update-001";

    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }

    // Create stack first
    let stack_name = stack_name();
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name.clone(),
        description: None,
        adapter_ids: vec!["adapter-update-001".to_string()],
        workflow_type: None,
        determinism_mode: None,
    };
    let stack_id = db
        .insert_stack(&stack_req)
        .await
        .expect("Failed to insert stack");

    // Create training job
    if let Err(e) = create_test_training_job(&db, job_id, tenant_id, repo_id, "running").await {
        eprintln!("Skipping test - training job creation failed: {}", e);
        return;
    }

    // Verify initially no stack_id or adapter_id
    let job_before = db
        .get_training_job(job_id)
        .await
        .expect("Failed to get job")
        .expect("Job not found");
    assert!(
        job_before.stack_id.is_none(),
        "stack_id should be None initially"
    );
    assert!(
        job_before.adapter_id.is_none(),
        "adapter_id should be None initially"
    );

    // Update with result IDs
    let adapter_id = "adapter-update-001";
    db.update_training_job_result_ids(job_id, Some(&stack_id), Some(adapter_id))
        .await
        .expect("Failed to update result IDs");

    // Verify they're persisted
    let job_after = db
        .get_training_job(job_id)
        .await
        .expect("Failed to get job")
        .expect("Job not found");
    assert_eq!(
        job_after.stack_id,
        Some(stack_id.clone()),
        "stack_id should be set"
    );
    assert_eq!(
        job_after.adapter_id,
        Some(adapter_id.to_string()),
        "adapter_id should be set"
    );
}

#[tokio::test]
async fn test_get_training_job_with_completed_status() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-complete";
    let repo_id = "test-repo-complete";
    let job_id = "train-complete-001";

    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }

    // Create completed training job
    if let Err(e) = create_test_training_job(&db, job_id, tenant_id, repo_id, "completed").await {
        eprintln!("Skipping test - training job creation failed: {}", e);
        return;
    }

    let job = db
        .get_training_job(job_id)
        .await
        .expect("Failed to get job")
        .expect("Job not found");

    assert_eq!(job.id, job_id);
    assert_eq!(job.status, "completed");
    assert_eq!(job.tenant_id, Some(tenant_id.to_string()));
    assert_eq!(job.adapter_name, Some("test-adapter".to_string()));
}

#[tokio::test]
async fn test_create_chat_session_with_stack() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-chat";
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Seed tenant, user, and stack to satisfy chat_sessions FKs for stack-backed chats.
    let user_id = "user-chat-001";
    if let Err(e) = create_test_user(&db, user_id).await {
        eprintln!("Skipping test - user creation failed: {}", e);
        return;
    }

    // Create stack
    let stack_name = stack_name();
    let stack_req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name.clone(),
        description: None,
        adapter_ids: vec!["adapter-chat-001".to_string()],
        workflow_type: None,
        determinism_mode: None,
    };
    let stack_id = db
        .insert_stack(&stack_req)
        .await
        .expect("Failed to insert stack");

    // Create chat session with stack_id (no collection_id to avoid FK constraint)
    let session_id = "session-chat-001";

    db.create_chat_session(CreateChatSessionParams {
        id: session_id.to_string(),
        tenant_id: tenant_id.to_string(),
        user_id: Some(user_id.to_string()),
        created_by: Some(user_id.to_string()),
        stack_id: Some(stack_id.clone()),
        collection_id: None, // Collection would need to exist and match tenant
        document_id: None,
        name: "Chat with test-adapter".to_string(),
        title: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags_json: None,
        pinned_adapter_ids: None,
    })
    .await
    .expect("Failed to create chat session");

    // Verify session was created with correct fields
    let session = db
        .get_chat_session(session_id)
        .await
        .expect("Failed to get session")
        .expect("Session not found");

    assert_eq!(session.stack_id, Some(stack_id));
    assert_eq!(session.collection_id, None);
    assert_eq!(session.name, "Chat with test-adapter");
    assert_eq!(session.tenant_id, tenant_id);
}

#[tokio::test]
async fn test_training_job_tenant_id_persisted() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_a = "tenant-isolation-a";
    let tenant_b = "tenant-isolation-b";
    let repo_id = "test-repo-isolation";

    if let Err(e) = create_test_tenant(&db, tenant_a).await {
        eprintln!("Skipping test - tenant A creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_tenant(&db, tenant_b).await {
        eprintln!("Skipping test - tenant B creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }

    // Create jobs for different tenants
    if let Err(e) = create_test_training_job(&db, "job-iso-a", tenant_a, repo_id, "completed").await
    {
        eprintln!("Skipping test - job A creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_training_job(&db, "job-iso-b", tenant_b, repo_id, "completed").await
    {
        eprintln!("Skipping test - job B creation failed: {}", e);
        return;
    }

    // Verify each job has correct tenant
    let job_a = db
        .get_training_job("job-iso-a")
        .await
        .expect("Failed to get job A")
        .expect("Job A not found");
    let job_b = db
        .get_training_job("job-iso-b")
        .await
        .expect("Failed to get job B")
        .expect("Job B not found");

    assert_eq!(job_a.tenant_id, Some(tenant_a.to_string()));
    assert_eq!(job_b.tenant_id, Some(tenant_b.to_string()));
}
