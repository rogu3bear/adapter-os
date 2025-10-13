use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

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
    pub async fn create_patch_proposal(
        &self,
        repo_id: &str,
        commit_sha: &str,
        description: &str,
        target_files_json: &str,
        patch_json: &str,
        validation_result_json: &str,
        status: &str,
        created_by: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO patch_proposals 
             (id, repo_id, commit_sha, description, target_files_json, patch_json, validation_result_json, status, created_by) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(repo_id)
        .bind(commit_sha)
        .bind(description)
        .bind(target_files_json)
        .bind(patch_json)
        .bind(validation_result_json)
        .bind(status)
        .bind(created_by)
        .execute(self.pool())
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
        .fetch_optional(self.pool())
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
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query_as::<_, PatchProposal>(
                "SELECT id, repo_id, commit_sha, description, target_files_json, patch_json, 
                        validation_result_json, status, created_at, created_by 
                 FROM patch_proposals ORDER BY created_at DESC",
            )
            .fetch_all(self.pool())
            .await?
        };
        Ok(proposals)
    }

    pub async fn update_patch_proposal_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE patch_proposals SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
