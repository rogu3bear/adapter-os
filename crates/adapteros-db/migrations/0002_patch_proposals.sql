-- Patch proposals table
CREATE TABLE IF NOT EXISTS patch_proposals (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    description TEXT NOT NULL,
    target_files_json TEXT NOT NULL,
    patch_json TEXT NOT NULL,
    validation_result_json TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    created_by TEXT NOT NULL
);

-- Index for faster lookups
CREATE INDEX IF NOT EXISTS idx_patch_proposals_repo ON patch_proposals(repo_id, commit_sha);
CREATE INDEX IF NOT EXISTS idx_patch_proposals_status ON patch_proposals(status);
CREATE INDEX IF NOT EXISTS idx_patch_proposals_created_by ON patch_proposals(created_by);
