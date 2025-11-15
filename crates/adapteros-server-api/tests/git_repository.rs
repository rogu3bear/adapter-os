use std::sync::Arc;

use adapteros_api_types::repositories::RegisterRepositoryRequest;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::handlers::git_repository::register_git_repository;
use axum::extract::State;
use axum::{Extension, Json};
use tempfile::TempDir;

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn register_repository_registers_branch_manager() -> anyhow::Result<()> {
    // Create temporary git repository with initial commit on main branch
    let temp_dir = TempDir::new()?;
    let repo_path = temp_dir.path().join("git_repo");
    std::fs::create_dir_all(&repo_path)?;

    let repo = git2::Repository::init(&repo_path)?;
    std::fs::write(repo_path.join("README.md"), "test repo")?;

    let mut index = repo.index()?;
    index.add_path(std::path::Path::new("README.md"))?;
    index.write()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = git2::Signature::now("adapteros", "adapteros@example.com")?;
    let commit_id = repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initial commit",
        &tree,
        &[],
    )?;
    let commit = repo.find_commit(commit_id)?;
    repo.branch("main", &commit, true)?;
    repo.set_head("refs/heads/main")?;

    // Prepare AppState with Git subsystem enabled
    let mut state = setup_state(None).await?;
    if let adapteros_db::DatabaseBackend::Sqlite(db) = state.db.backend() {
        db.migrate().await?;
    }

    let (file_change_tx, _rx) = tokio::sync::broadcast::channel(16);
    let git_config = adapteros_git::GitConfig { enabled: true };
    let git_subsystem =
        Arc::new(adapteros_git::GitSubsystem::new(git_config, state.db.clone()).await?);
    state.git_subsystem = Some(git_subsystem.clone());
    state.file_change_tx = Some(Arc::new(file_change_tx));

    let claims: Claims = test_admin_claims();
    let request: RegisterRepositoryRequest = serde_json::from_value(serde_json::json!({
        "url": format!("file://{}", repo_path.display()),
        "branch": "main"
    }))?;

    // Register repository via handler
    let Json(response) = register_git_repository(
        State(state.clone()),
        Extension(claims),
        Json(request.clone()),
    )
    .await
    .map_err(|(status, err_json)| {
        anyhow::anyhow!(format!(
            "handler error {}: {}",
            status,
            serde_json::to_string(&err_json.0).unwrap_or_default()
        ))
    })?;

    assert_eq!(response.analysis.git_info.branch, "main");
    assert_eq!(response.analysis.git_info.commit_count, 1);
    
    // BranchManager should now know about the repository so sessions can start
    let session = git_subsystem
        .branch_manager()
        .start_session(
            "adapter-123".to_string(),
            response.repo_id.clone(),
            Some(response.analysis.git_info.branch.clone()),
        )
        .await?;

    assert_eq!(session.repo_id, response.repo_id);

    // Repository path stored by BranchManager must resolve to canonical path
    let registered_path = git_subsystem
        .branch_manager()
        .get_repository_path(&response.repo_id)
        .await
        .expect("repository should be registered");
    let canonical_path = std::fs::canonicalize(&repo_path)?;
    assert_eq!(registered_path, canonical_path);

    Ok(())
}
