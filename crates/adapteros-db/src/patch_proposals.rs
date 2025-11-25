use crate::Db;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Builder for creating patch proposal parameters
#[derive(Debug, Default)]
pub struct PatchProposalBuilder {
    repo_id: Option<String>,
    commit_sha: Option<String>,
    description: Option<String>,
    target_files_json: Option<String>,
    patch_json: Option<String>,
    validation_result_json: Option<String>,
    status: Option<String>,
    created_by: Option<String>,
}

/// Parameters for patch proposal creation
#[derive(Debug)]
pub struct PatchProposalParams {
    pub repo_id: String,
    pub commit_sha: String,
    pub description: String,
    pub target_files_json: String,
    pub patch_json: String,
    pub validation_result_json: String,
    pub status: String,
    pub created_by: String,
}

impl PatchProposalBuilder {
    /// Create a new patch proposal builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the repository ID (required)
    pub fn repo_id(mut self, repo_id: impl Into<String>) -> Self {
        self.repo_id = Some(repo_id.into());
        self
    }

    /// Set the commit SHA (required)
    pub fn commit_sha(mut self, commit_sha: impl Into<String>) -> Self {
        self.commit_sha = Some(commit_sha.into());
        self
    }

    /// Set the description (required)
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the target files JSON (required)
    pub fn target_files_json(mut self, target_files_json: impl Into<String>) -> Self {
        self.target_files_json = Some(target_files_json.into());
        self
    }

    /// Set the patch JSON (required)
    pub fn patch_json(mut self, patch_json: impl Into<String>) -> Self {
        self.patch_json = Some(patch_json.into());
        self
    }

    /// Set the validation result JSON (required)
    pub fn validation_result_json(mut self, validation_result_json: impl Into<String>) -> Self {
        self.validation_result_json = Some(validation_result_json.into());
        self
    }

    /// Set the status (required)
    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    /// Set the created by user (required)
    pub fn created_by(mut self, created_by: impl Into<String>) -> Self {
        self.created_by = Some(created_by.into());
        self
    }

    /// Build the patch proposal parameters
    pub fn build(self) -> Result<PatchProposalParams> {
        Ok(PatchProposalParams {
            repo_id: self.repo_id.ok_or_else(|| anyhow!("repo_id is required"))?,
            commit_sha: self
                .commit_sha
                .ok_or_else(|| anyhow!("commit_sha is required"))?,
            description: self
                .description
                .ok_or_else(|| anyhow!("description is required"))?,
            target_files_json: self
                .target_files_json
                .ok_or_else(|| anyhow!("target_files_json is required"))?,
            patch_json: self
                .patch_json
                .ok_or_else(|| anyhow!("patch_json is required"))?,
            validation_result_json: self
                .validation_result_json
                .ok_or_else(|| anyhow!("validation_result_json is required"))?,
            status: self.status.ok_or_else(|| anyhow!("status is required"))?,
            created_by: self
                .created_by
                .ok_or_else(|| anyhow!("created_by is required"))?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PatchProposal {
    pub id: String,
    pub repo_id: String,
    pub commit_sha: String,
    pub description: String,
    pub target_files_json: String,
    pub patch_json: String,
    pub validation_result_json: String,
    pub status: String,
    pub created_at: String,
    pub created_by: String,
}

impl Db {
    /// Create patch proposal
    ///
    /// Use [`PatchProposalBuilder`] to construct complex parameter sets:
    /// ```no_run
    /// use adapteros_db::patch_proposals::PatchProposalBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = PatchProposalBuilder::new()
    ///     .repo_id("github.com/org/repo")
    ///     .commit_sha("a1b2c3d4...")
    ///     .description("Fix critical security vulnerability")
    ///     .target_files_json(r#"["src/main.rs", "src/auth.rs"]"#)
    ///     .patch_json(r#"[{"file": "src/main.rs", "changes": "..."}]"#)
    ///     .validation_result_json(r#"{"passed": true, "tests": 42}"#)
    ///     .status("proposed")
    ///     .created_by("developer@example.com")
    ///     .build()
    ///     .expect("required fields");
    /// db.create_patch_proposal(params).await.expect("creation succeeds");
    /// # }
    /// ```
    pub async fn create_patch_proposal(&self, params: PatchProposalParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO patch_proposals 
             (id, repo_id, commit_sha, description, target_files_json, patch_json, validation_result_json, status, created_by) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&params.repo_id)
        .bind(&params.commit_sha)
        .bind(&params.description)
        .bind(&params.target_files_json)
        .bind(&params.patch_json)
        .bind(&params.validation_result_json)
        .bind(&params.status)
        .bind(&params.created_by)
        .execute(&*self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_patch_proposal(&self, id: &str) -> Result<Option<PatchProposal>> {
        let proposal = sqlx::query_as::<_, PatchProposal>(
            "SELECT id, repo_id, commit_sha, description, target_files_json, patch_json, 
                    validation_result_json, status, created_at, created_by 
             FROM patch_proposals WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&*self.pool())
        .await?;
        Ok(proposal)
    }

    pub async fn list_patch_proposals(&self, repo_id: Option<&str>) -> Result<Vec<PatchProposal>> {
        let proposals = if let Some(repo) = repo_id {
            sqlx::query_as::<_, PatchProposal>(
                "SELECT id, repo_id, commit_sha, description, target_files_json, patch_json, 
                        validation_result_json, status, created_at, created_by 
                 FROM patch_proposals WHERE repo_id = ? ORDER BY created_at DESC",
            )
            .bind(repo)
            .fetch_all(&*self.pool())
            .await?
        } else {
            sqlx::query_as::<_, PatchProposal>(
                "SELECT id, repo_id, commit_sha, description, target_files_json, patch_json, 
                        validation_result_json, status, created_at, created_by 
                 FROM patch_proposals ORDER BY created_at DESC",
            )
            .fetch_all(&*self.pool())
            .await?
        };
        Ok(proposals)
    }

    pub async fn update_patch_proposal_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE patch_proposals SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(&*self.pool())
            .await?;
        Ok(())
    }
}
