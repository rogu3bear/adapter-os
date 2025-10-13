//! Documentation index using SQLite FTS5 for full-text search

use crate::evidence::{DocIndex, EvidenceSpan, EvidenceType};
use anyhow::Result;
use adapteros_core::B3Hash;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::Row;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

/// Documentation index implementation using SQLite FTS5
pub struct DocIndexImpl {
    pool: SqlitePool,
}

impl DocIndexImpl {
    /// Create a new documentation index
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(options).await?;

        // Create FTS5 table for documentation search
        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
                title,
                content,
                section,
                content='docs',
                content_rowid='id'
            )",
        )
        .execute(&pool)
        .await?;

        // Create main docs table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS docs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                doc_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                file_path TEXT NOT NULL,
                section TEXT,
                line_start INTEGER,
                line_end INTEGER,
                repo_id TEXT,
                commit_sha TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        // Create triggers for FTS5 sync
        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS docs_ai AFTER INSERT ON docs BEGIN
                INSERT INTO docs_fts(rowid, title, content, section)
                VALUES (new.id, new.title, new.content, new.section);
            END",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS docs_ad AFTER DELETE ON docs BEGIN
                INSERT INTO docs_fts(docs_fts, rowid, title, content, section)
                VALUES('delete', old.id, old.title, old.content, old.section);
            END",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// Index a documentation entry
    pub async fn index_doc(
        &self,
        doc_id: &str,
        title: &str,
        content: &str,
        file_path: &str,
        section: Option<&str>,
        line_start: usize,
        line_end: usize,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO docs (doc_id, title, content, file_path, section, line_start, line_end, repo_id, commit_sha)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(doc_id)
        .bind(title)
        .bind(content)
        .bind(file_path)
        .bind(section)
        .bind(line_start as i64)
        .bind(line_end as i64)
        .bind(repo_id)
        .bind(commit_sha)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Clear index for a repository
    pub async fn clear_repo(&self, repo_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM docs WHERE repo_id = ?")
            .bind(repo_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Search documentation using FTS5
    pub async fn search_async(&self, query: &str, limit: usize) -> Result<Vec<EvidenceSpan>> {
        let rows = sqlx::query(
            "SELECT d.doc_id, d.title, d.content, d.file_path, d.section, d.line_start, d.line_end,
                    d.repo_id, d.commit_sha, rank
             FROM docs_fts df
             JOIN docs d ON df.rowid = d.id
             WHERE docs_fts MATCH ?
             ORDER BY rank
             LIMIT ?",
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let doc_id: String = row.try_get("doc_id")?;
            let title: String = row.try_get("title")?;
            let content: String = row.try_get("content")?;
            let file_path: String = row.try_get("file_path")?;
            let section: Option<String> = row.try_get("section").ok();
            let line_start: i64 = row.try_get("line_start")?;
            let line_end: i64 = row.try_get("line_end")?;
            let repo_id: String = row.try_get("repo_id")?;
            let commit_sha: String = row.try_get("commit_sha")?;
            let rank: f64 = row.try_get("rank")?;

            // Compute span hash from content
            let span_content = format!("# {}\n\n{}", title, content);
            let span_hash = B3Hash::hash(span_content.as_bytes()).to_hex();

            let mut metadata = HashMap::new();
            metadata.insert("doc_id".to_string(), doc_id);
            metadata.insert("title".to_string(), title);
            if let Some(sec) = section {
                metadata.insert("section".to_string(), sec);
            }

            results.push(EvidenceSpan {
                doc_id: repo_id,
                rev: commit_sha,
                span_hash,
                score: (-rank as f32).exp(), // Convert FTS5 rank to similarity score
                evidence_type: EvidenceType::Doc,
                file_path,
                start_line: line_start as usize,
                end_line: line_end as usize,
                content: span_content,
                metadata,
            });
        }

        Ok(results)
    }

    /// Get documentation by ID
    pub async fn get_by_id(&self, doc_id: &str) -> Result<Option<EvidenceSpan>> {
        let row = sqlx::query(
            "SELECT doc_id, title, content, file_path, section, line_start, line_end, repo_id, commit_sha
             FROM docs
             WHERE doc_id = ?",
        )
        .bind(doc_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let doc_id: String = row.try_get("doc_id")?;
            let title: String = row.try_get("title")?;
            let content: String = row.try_get("content")?;
            let file_path: String = row.try_get("file_path")?;
            let section: Option<String> = row.try_get("section").ok();
            let line_start: i64 = row.try_get("line_start")?;
            let line_end: i64 = row.try_get("line_end")?;
            let repo_id: String = row.try_get("repo_id")?;
            let commit_sha: String = row.try_get("commit_sha")?;

            let span_content = format!("# {}\n\n{}", title, content);
            let span_hash = B3Hash::hash(span_content.as_bytes()).to_hex();

            let mut metadata = HashMap::new();
            metadata.insert("doc_id".to_string(), doc_id);
            metadata.insert("title".to_string(), title);
            if let Some(sec) = section {
                metadata.insert("section".to_string(), sec);
            }

            Ok(Some(EvidenceSpan {
                doc_id: repo_id,
                rev: commit_sha,
                span_hash,
                score: 1.0,
                evidence_type: EvidenceType::Doc,
                file_path,
                start_line: line_start as usize,
                end_line: line_end as usize,
                content: span_content,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }
}

impl DocIndex for DocIndexImpl {
    fn search(&self, query: &str) -> Result<Vec<EvidenceSpan>> {
        // Synchronous wrapper for async search
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { self.search_async(query, 10).await })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_doc_index() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("docs.db");

        let index = DocIndexImpl::new(db_path).await.unwrap();

        // Index a document
        index
            .index_doc(
                "doc_001",
                "Getting Started",
                "This is a guide on how to use the my_function API.",
                "docs/README.md",
                Some("Introduction"),
                1,
                10,
                "test-repo",
                "abc123",
            )
            .await
            .unwrap();

        // Search for documentation
        let results = index.search_async("my_function API", 10).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].metadata.get("title").unwrap(), "Getting Started");

        // Get by ID
        let doc = index.get_by_id("doc_001").await.unwrap();
        assert!(doc.is_some());
    }
}
