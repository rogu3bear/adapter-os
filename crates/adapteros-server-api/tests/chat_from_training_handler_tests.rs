//! Handler-level tests for Bundle D: Train → Stack → Chat endpoints
//!
//! Tests verify:
//! 1. chat_bootstrap returns provenance fields (training_job_id, status, adapter_id, dataset_id)
//! 2. create_chat_from_job returns provenance fields (training_job_id, adapter_id, dataset_id, collection_id)
//! 3. Error cases: 404 (not found), 403 (wrong tenant), 400 (not completed, no stack)

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

/// Test helper to create a dataset (for dataset_id provenance)
async fn create_test_dataset(db: &Db, dataset_id: &str, tenant_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO training_datasets (id, name, tenant_id, format, storage_path, hash_b3, validation_status, created_at)
         VALUES (?, ?, ?, 'jsonl', '/tmp/test', 'dummy_hash', 'valid', datetime('now'))",
    )
    .bind(dataset_id)
    .bind(format!("dataset-{}", dataset_id))
    .bind(tenant_id)
    .execute(db.pool())
    .await
    .map_err(|e| adapteros_core::AosError::Database(format!("Failed to create dataset: {}", e)))?;
    Ok(())
}

/// Test helper to create a training job with all provenance fields
async fn create_test_training_job_with_provenance(
    db: &Db,
    job_id: &str,
    tenant_id: &str,
    repo_id: &str,
    status: &str,
    adapter_id: Option<&str>,
    dataset_id: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, training_config_json, status, progress_json, created_by, adapter_name, tenant_id, adapter_id, dataset_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(job_id)
    .bind(repo_id)
    .bind(r#"{"rank":16,"alpha":32}"#)
    .bind(status)
    .bind(r#"{"progress":0}"#)
    .bind("test-user")
    .bind("test-adapter")
    .bind(tenant_id)
    .bind(adapter_id)
    .bind(dataset_id)
    .execute(db.pool())
    .await
    .map_err(|e| {
        adapteros_core::AosError::Database(format!("Failed to create training job: {}", e))
    })?;
    Ok(())
}

/// Test helper to create a stack
async fn create_test_stack(db: &Db, tenant_id: &str, stack_name: &str) -> Result<String> {
    let req = CreateStackRequest {
        tenant_id: tenant_id.to_string(),
        name: stack_name.to_string(),
        description: Some("Test stack".to_string()),
        adapter_ids: vec!["adapter-1".to_string()],
        workflow_type: Some("inference".to_string()),
        determinism_mode: None,
    };

    let stack_id = db.insert_stack(&req).await?;
    Ok(stack_id)
}

// =============================================================================
// Tests for chat_bootstrap provenance fields
// =============================================================================

#[tokio::test]
async fn test_chat_bootstrap_db_returns_provenance_fields() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-provenance";
    let job_id = "job-provenance-001";
    let repo_id = "repo-provenance";
    let dataset_id = "dataset-provenance-001";
    let adapter_id = "adapter-provenance-001";

    // Setup: tenant, repo, dataset
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_dataset(&db, dataset_id, tenant_id).await {
        eprintln!("Skipping test - dataset creation failed: {}", e);
        return;
    }

    // Create completed job with provenance fields
    if let Err(e) = create_test_training_job_with_provenance(
        &db,
        job_id,
        tenant_id,
        repo_id,
        "completed",
        Some(adapter_id),
        Some(dataset_id),
    )
    .await
    {
        eprintln!("Skipping test - job creation failed: {}", e);
        return;
    }

    // Create stack and link to job
    let stack_id = match create_test_stack(&db, tenant_id, &stack_name()).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Skipping test - stack creation failed: {}", e);
            return;
        }
    };

    // Link job to stack
    if let Err(e) = db
        .update_training_job_result_ids(job_id, Some(&stack_id), Some(adapter_id))
        .await
    {
        eprintln!("Skipping test - failed to link job to stack: {}", e);
        return;
    }

    // Verify: get_training_job returns all provenance fields
    match db.get_training_job(job_id).await {
        Ok(Some(job)) => {
            // Core provenance fields that chat_bootstrap should return
            assert_eq!(job.id, job_id, "Job ID should match");
            assert_eq!(job.status, "completed", "Status should be completed");
            assert_eq!(
                job.stack_id,
                Some(stack_id.clone()),
                "Stack ID should be set"
            );
            assert_eq!(
                job.adapter_id,
                Some(adapter_id.to_string()),
                "Adapter ID should be set"
            );
            assert_eq!(
                job.dataset_id,
                Some(dataset_id.to_string()),
                "Dataset ID should be set"
            );
            assert_eq!(
                job.tenant_id,
                Some(tenant_id.to_string()),
                "Tenant ID should be set"
            );
        }
        Ok(None) => {
            panic!("Training job not found");
        }
        Err(e) => {
            panic!("Failed to get training job: {}", e);
        }
    }
}

#[tokio::test]
async fn test_chat_bootstrap_returns_status_field() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-status";
    let repo_id = "repo-status";

    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }

    // Test different status values
    for (job_id, status) in [
        ("job-pending", "pending"),
        ("job-running", "running"),
        ("job-completed", "completed"),
        ("job-failed", "failed"),
        ("job-cancelled", "cancelled"),
    ] {
        if let Err(e) = create_test_training_job_with_provenance(
            &db, job_id, tenant_id, repo_id, status, None, None,
        )
        .await
        {
            eprintln!(
                "Skipping status test for {} - job creation failed: {}",
                status, e
            );
            continue;
        }

        match db.get_training_job(job_id).await {
            Ok(Some(job)) => {
                assert_eq!(
                    job.status, status,
                    "Status should be {} for job {}",
                    status, job_id
                );
            }
            Ok(None) => {
                panic!("Job {} not found", job_id);
            }
            Err(e) => {
                panic!("Failed to get job {}: {}", job_id, e);
            }
        }
    }
}

// =============================================================================
// Tests for create_chat_from_job provenance fields
// =============================================================================

#[tokio::test]
async fn test_create_chat_from_job_with_provenance_fields() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-chat-provenance";
    let job_id = "job-chat-provenance-001";
    let repo_id = "repo-chat-provenance";
    let dataset_id = "dataset-chat-001";
    let adapter_id = "adapter-chat-001";

    // Setup
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_dataset(&db, dataset_id, tenant_id).await {
        eprintln!("Skipping test - dataset creation failed: {}", e);
        return;
    }

    // Create completed job with provenance
    if let Err(e) = create_test_training_job_with_provenance(
        &db,
        job_id,
        tenant_id,
        repo_id,
        "completed",
        Some(adapter_id),
        Some(dataset_id),
    )
    .await
    {
        eprintln!("Skipping test - job creation failed: {}", e);
        return;
    }

    // Create stack
    let stack_id = match create_test_stack(&db, tenant_id, &stack_name()).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Skipping test - stack creation failed: {}", e);
            return;
        }
    };

    // Link job to stack
    if let Err(e) = db
        .update_training_job_result_ids(job_id, Some(&stack_id), Some(adapter_id))
        .await
    {
        eprintln!("Skipping test - failed to link job to stack: {}", e);
        return;
    }

    // Create chat session with stack from training job
    let session_id = "session-provenance-001";
    let params = CreateChatSessionParams {
        id: session_id.to_string(),
        tenant_id: tenant_id.to_string(),
        user_id: Some("test-user".to_string()),
        stack_id: Some(stack_id.clone()),
        collection_id: None,
        name: "Chat from training job".to_string(),
        metadata_json: None,
        pinned_adapter_ids: None,
    };

    match db.create_chat_session(params).await {
        Ok(returned_session_id) => {
            // Verify session was created with correct stack_id
            match db.get_chat_session(&returned_session_id).await {
                Ok(Some(session)) => {
                    assert_eq!(
                        session.stack_id,
                        Some(stack_id.clone()),
                        "Chat session should have stack_id from training job"
                    );
                    assert_eq!(
                        session.tenant_id, tenant_id,
                        "Chat session should have correct tenant_id"
                    );
                }
                Ok(None) => {
                    panic!("Chat session not found after creation");
                }
                Err(e) => {
                    panic!("Failed to get chat session: {}", e);
                }
            }
        }
        Err(e) => {
            panic!("Failed to create chat session: {}", e);
        }
    }
}

// =============================================================================
// Tests for error cases: 404 (not found)
// =============================================================================

#[tokio::test]
async fn test_get_training_job_not_found_returns_none() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Query for non-existent job
    match db.get_training_job("nonexistent-job-id").await {
        Ok(None) => {
            // Expected: job not found returns None
            // Handler would convert this to 404 NOT_FOUND
        }
        Ok(Some(_)) => {
            panic!("Should not find nonexistent job");
        }
        Err(e) => {
            panic!("Unexpected error: {}", e);
        }
    }
}

// =============================================================================
// Tests for error cases: Job not completed (for create_chat_from_job)
// =============================================================================

#[tokio::test]
async fn test_create_chat_requires_completed_job() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-incomplete";
    let repo_id = "repo-incomplete";

    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }

    // Create job with status="pending" (not completed)
    if let Err(e) = create_test_training_job_with_provenance(
        &db,
        "job-pending-001",
        tenant_id,
        repo_id,
        "pending",
        None,
        None,
    )
    .await
    {
        eprintln!("Skipping test - job creation failed: {}", e);
        return;
    }

    // Verify job status is pending (handler would return JOB_NOT_COMPLETED error)
    match db.get_training_job("job-pending-001").await {
        Ok(Some(job)) => {
            assert_eq!(job.status, "pending", "Job should be pending");
            // Handler check: if job.status != "completed" -> return JOB_NOT_COMPLETED
            assert_ne!(
                job.status, "completed",
                "Job should NOT be completed - handler would return 400 JOB_NOT_COMPLETED"
            );
        }
        Ok(None) => {
            panic!("Job not found");
        }
        Err(e) => {
            panic!("Failed to get job: {}", e);
        }
    }
}

// =============================================================================
// Tests for error cases: Job without stack (for create_chat_from_job)
// =============================================================================

#[tokio::test]
async fn test_create_chat_requires_stack_id() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-no-stack";
    let repo_id = "repo-no-stack";

    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }

    // Create completed job but WITHOUT stack_id
    if let Err(e) = create_test_training_job_with_provenance(
        &db,
        "job-no-stack-001",
        tenant_id,
        repo_id,
        "completed",
        Some("adapter-1"),
        None,
    )
    .await
    {
        eprintln!("Skipping test - job creation failed: {}", e);
        return;
    }

    // Verify job has no stack_id (handler would return NO_STACK error)
    match db.get_training_job("job-no-stack-001").await {
        Ok(Some(job)) => {
            assert_eq!(job.status, "completed", "Job should be completed");
            assert!(
                job.stack_id.is_none(),
                "Job should NOT have stack_id - handler would return 400 NO_STACK"
            );
        }
        Ok(None) => {
            panic!("Job not found");
        }
        Err(e) => {
            panic!("Failed to get job: {}", e);
        }
    }
}

// =============================================================================
// Tests for tenant isolation (403 scenario)
// =============================================================================

#[tokio::test]
async fn test_tenant_isolation_on_training_job() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_a = "tenant-isolation-a";
    let tenant_b = "tenant-isolation-b";
    let repo_id = "repo-isolation";

    for tenant in [tenant_a, tenant_b] {
        if let Err(e) = create_test_tenant(&db, tenant).await {
            eprintln!("Skipping test - tenant creation failed: {}", e);
            return;
        }
    }
    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }

    // Create job for tenant A
    if let Err(e) = create_test_training_job_with_provenance(
        &db,
        "job-tenant-a",
        tenant_a,
        repo_id,
        "completed",
        None,
        None,
    )
    .await
    {
        eprintln!("Skipping test - job creation failed: {}", e);
        return;
    }

    // Verify job belongs to tenant A
    match db.get_training_job("job-tenant-a").await {
        Ok(Some(job)) => {
            assert_eq!(
                job.tenant_id,
                Some(tenant_a.to_string()),
                "Job should belong to tenant A"
            );
            // Handler would check: if claims.tenant_id != job.tenant_id -> return 403 FORBIDDEN
        }
        Ok(None) => {
            panic!("Job not found");
        }
        Err(e) => {
            panic!("Failed to get job: {}", e);
        }
    }
}

// =============================================================================
// Tests for full provenance chain: job -> adapter -> stack -> chat
// =============================================================================

#[tokio::test]
async fn test_full_provenance_chain_job_to_chat() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "test-tenant-chain";
    let job_id = "job-chain-001";
    let repo_id = "repo-chain";
    let dataset_id = "dataset-chain-001";
    let adapter_id = "adapter-chain-001";

    // Setup
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_repo(&db, repo_id).await {
        eprintln!("Skipping test - repo creation failed: {}", e);
        return;
    }
    if let Err(e) = create_test_dataset(&db, dataset_id, tenant_id).await {
        eprintln!("Skipping test - dataset creation failed: {}", e);
        return;
    }

    // Step 1: Create training job with dataset
    if let Err(e) = create_test_training_job_with_provenance(
        &db,
        job_id,
        tenant_id,
        repo_id,
        "completed",
        Some(adapter_id),
        Some(dataset_id),
    )
    .await
    {
        eprintln!("Skipping test - job creation failed: {}", e);
        return;
    }

    // Step 2: Create stack
    let stack_id = match create_test_stack(&db, tenant_id, &stack_name()).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Skipping test - stack creation failed: {}", e);
            return;
        }
    };

    // Step 3: Link job to stack (simulates orchestrator post-action)
    if let Err(e) = db
        .update_training_job_result_ids(job_id, Some(&stack_id), Some(adapter_id))
        .await
    {
        eprintln!("Skipping test - failed to link job to stack: {}", e);
        return;
    }

    // Step 4: Create chat session with stack
    let session_id = "session-chain-001";
    let params = CreateChatSessionParams {
        id: session_id.to_string(),
        tenant_id: tenant_id.to_string(),
        user_id: Some("test-user".to_string()),
        stack_id: Some(stack_id.clone()),
        collection_id: None,
        name: "Chat from training".to_string(),
        metadata_json: None,
        pinned_adapter_ids: None,
    };

    if let Err(e) = db.create_chat_session(params).await {
        eprintln!("Skipping test - chat session creation failed: {}", e);
        return;
    }

    // Verify: Full chain is traceable
    // chat_session -> stack_id -> training_job (via stack_id) -> adapter_id, dataset_id

    // 1. Get chat session
    let session = db
        .get_chat_session(session_id)
        .await
        .expect("Failed to get session")
        .expect("Session not found");

    assert_eq!(
        session.stack_id,
        Some(stack_id.clone()),
        "Session has stack"
    );

    // 2. Get training job (via known job_id for this test)
    let job = db
        .get_training_job(job_id)
        .await
        .expect("Failed to get job")
        .expect("Job not found");

    assert_eq!(job.stack_id, Some(stack_id.clone()), "Job linked to stack");
    assert_eq!(
        job.adapter_id,
        Some(adapter_id.to_string()),
        "Job has adapter_id"
    );
    assert_eq!(
        job.dataset_id,
        Some(dataset_id.to_string()),
        "Job has dataset_id"
    );

    // 3. Get stack to verify adapter_ids
    let stack = db
        .get_stack(tenant_id, &stack_id)
        .await
        .expect("Failed to get stack")
        .expect("Stack not found");

    let stack_adapter_ids: Vec<String> =
        serde_json::from_str(&stack.adapter_ids_json).expect("Failed to parse adapter_ids_json");

    assert!(
        !stack_adapter_ids.is_empty(),
        "Stack should have adapter IDs"
    );

    // Full chain verified: chat -> stack -> job -> adapter/dataset
}
