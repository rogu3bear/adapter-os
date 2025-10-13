//! Symbol index using SQLite FTS5 for full-text search

use crate::evidence::{EvidenceSpan, EvidenceType, SymbolIndex};
use anyhow::Result;
use adapteros_core::B3Hash;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::Row;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

/// Symbol index implementation using SQLite FTS5
pub struct SymbolIndexImpl {
    pool: SqlitePool,
}

impl SymbolIndexImpl {
    /// Create a new symbol index
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(options).await?;

        // Create FTS5 table for symbol search
        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
                symbol_name,
                file_path,
                signature,
                docstring,
                content='symbols',
                content_rowid='id'
            )",
        )
        .execute(&pool)
        .await?;

        // Create main symbols table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line_number INTEGER NOT NULL,
                signature TEXT,
                docstring TEXT,
                repo_id TEXT,
                commit_sha TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        // Create triggers for FTS5 sync
        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS symbols_ai AFTER INSERT ON symbols BEGIN
                INSERT INTO symbols_fts(rowid, symbol_name, file_path, signature, docstring)
                VALUES (new.id, new.symbol_name, new.file_path, new.signature, new.docstring);
            END",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS symbols_ad AFTER DELETE ON symbols BEGIN
                INSERT INTO symbols_fts(symbols_fts, rowid, symbol_name, file_path, signature, docstring)
                VALUES('delete', old.id, old.symbol_name, old.file_path, old.signature, old.docstring);
            END",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// Index a symbol
    pub async fn index_symbol(
        &self,
        symbol_name: &str,
        file_path: &str,
        line_number: usize,
        signature: Option<&str>,
        docstring: Option<&str>,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO symbols (symbol_name, file_path, line_number, signature, docstring, repo_id, commit_sha)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(symbol_name)
        .bind(file_path)
        .bind(line_number as i64)
        .bind(signature)
        .bind(docstring)
        .bind(repo_id)
        .bind(commit_sha)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Clear index for a repository
    pub async fn clear_repo(&self, repo_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM symbols WHERE repo_id = ?")
            .bind(repo_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Search symbols using FTS5
    pub async fn search_async(&self, query: &str, limit: usize) -> Result<Vec<EvidenceSpan>> {
        let rows = sqlx::query(
            "SELECT s.symbol_name, s.file_path, s.line_number, s.signature, s.docstring, s.repo_id, s.commit_sha,
                    rank
             FROM symbols_fts sf
             JOIN symbols s ON sf.rowid = s.id
             WHERE symbols_fts MATCH ?
             ORDER BY rank
             LIMIT ?",
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let symbol_name: String = row.try_get("symbol_name")?;
            let file_path: String = row.try_get("file_path")?;
            let line_number: i64 = row.try_get("line_number")?;
            let signature: Option<String> = row.try_get("signature").ok();
            let docstring: Option<String> = row.try_get("docstring").ok();
            let repo_id: String = row.try_get("repo_id")?;
            let commit_sha: String = row.try_get("commit_sha")?;
            let rank: f64 = row.try_get("rank")?;

            // Compute span hash from content
            let content = format!(
                "{}\n{}",
                signature.as_deref().unwrap_or(""),
                docstring.as_deref().unwrap_or("")
            );
            let span_hash = B3Hash::hash(content.as_bytes()).to_hex();

            let mut metadata = HashMap::new();
            metadata.insert("symbol_name".to_string(), symbol_name.clone());
            if let Some(sig) = signature.clone() {
                metadata.insert("signature".to_string(), sig);
            }

            results.push(EvidenceSpan {
                doc_id: repo_id,
                rev: commit_sha,
                span_hash,
                score: (-rank as f32).exp(), // Convert FTS5 rank to similarity score
                evidence_type: EvidenceType::Symbol,
                file_path,
                start_line: line_number as usize,
                end_line: line_number as usize,
                content,
                metadata,
            });
        }

        Ok(results)
    }
}

impl SymbolIndex for SymbolIndexImpl {
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
    async fn test_symbol_index() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("symbols.db");

        let index = SymbolIndexImpl::new(db_path).await.unwrap();

        // Index some symbols
        index
            .index_symbol(
                "my_function",
                "src/main.rs",
                42,
                Some("fn my_function() -> i32"),
                Some("A test function"),
                "test-repo",
                "abc123",
            )
            .await
            .unwrap();

        // Search for symbols
        let results = index.search_async("my_function", 10).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(
            results[0].metadata.get("symbol_name").unwrap(),
            "my_function"
        );
    }
}
