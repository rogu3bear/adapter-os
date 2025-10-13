//! Test index using SQLite for test-to-code mapping

use crate::evidence::{EvidenceSpan, EvidenceType, TestIndex};
use anyhow::Result;
use adapteros_core::B3Hash;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::Row;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

/// Test index implementation using SQLite
pub struct TestIndexImpl {
    pool: SqlitePool,
}

impl TestIndexImpl {
    /// Create a new test index
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(options).await?;

        // Create tests table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                test_id TEXT NOT NULL UNIQUE,
                test_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                framework TEXT NOT NULL,
                repo_id TEXT,
                commit_sha TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        // Create test_coverage table for test-to-symbol mapping
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS test_coverage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                test_id TEXT NOT NULL,
                symbol_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                FOREIGN KEY (test_id) REFERENCES tests(test_id)
            )",
        )
        .execute(&pool)
        .await?;

        // Create index for faster lookups
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_test_coverage_symbol ON test_coverage(symbol_name)",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// Index a test
    pub async fn index_test(
        &self,
        test_id: &str,
        test_name: &str,
        file_path: &str,
        framework: &str,
        covered_symbols: &[String],
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<()> {
        // Insert test
        sqlx::query(
            "INSERT OR REPLACE INTO tests (test_id, test_name, file_path, framework, repo_id, commit_sha)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(test_id)
        .bind(test_name)
        .bind(file_path)
        .bind(framework)
        .bind(repo_id)
        .bind(commit_sha)
        .execute(&self.pool)
        .await?;

        // Delete old coverage entries
        sqlx::query("DELETE FROM test_coverage WHERE test_id = ?")
            .bind(test_id)
            .execute(&self.pool)
            .await?;

        // Insert coverage entries
        for symbol in covered_symbols {
            sqlx::query(
                "INSERT INTO test_coverage (test_id, symbol_name, file_path)
                 VALUES (?, ?, ?)",
            )
            .bind(test_id)
            .bind(symbol)
            .bind(file_path)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Clear index for a repository
    pub async fn clear_repo(&self, repo_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM tests WHERE repo_id = ?")
            .bind(repo_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Search tests by covered symbols
    pub async fn search_async(&self, query: &str, limit: usize) -> Result<Vec<EvidenceSpan>> {
        let rows = sqlx::query(
            "SELECT DISTINCT t.test_id, t.test_name, t.file_path, t.framework, t.repo_id, t.commit_sha,
                    COUNT(tc.symbol_name) as coverage_count
             FROM tests t
             JOIN test_coverage tc ON t.test_id = tc.test_id
             WHERE tc.symbol_name LIKE ?
             GROUP BY t.test_id
             ORDER BY coverage_count DESC
             LIMIT ?",
        )
        .bind(format!("%{}%", query))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let test_id: String = row.try_get("test_id")?;
            let test_name: String = row.try_get("test_name")?;
            let file_path: String = row.try_get("file_path")?;
            let framework: String = row.try_get("framework")?;
            let repo_id: String = row.try_get("repo_id")?;
            let commit_sha: String = row.try_get("commit_sha")?;
            let coverage_count: i64 = row.try_get("coverage_count")?;

            let content = format!("Test: {}\nFramework: {}", test_name, framework);
            let span_hash = B3Hash::hash(content.as_bytes()).to_hex();

            let mut metadata = HashMap::new();
            metadata.insert("test_id".to_string(), test_id);
            metadata.insert("test_name".to_string(), test_name);
            metadata.insert("framework".to_string(), framework);
            metadata.insert("coverage_count".to_string(), coverage_count.to_string());

            results.push(EvidenceSpan {
                doc_id: repo_id,
                rev: commit_sha,
                span_hash,
                score: (coverage_count as f32).ln() + 1.0, // Log-scale scoring
                evidence_type: EvidenceType::Test,
                file_path,
                start_line: 1,
                end_line: 1,
                content,
                metadata,
            });
        }

        Ok(results)
    }

    /// Get tests covering a specific symbol
    pub async fn get_tests_for_symbol(&self, symbol_name: &str) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT DISTINCT test_id FROM test_coverage WHERE symbol_name = ?")
            .bind(symbol_name)
            .fetch_all(&self.pool)
            .await?;

        let mut test_ids = Vec::new();
        for row in rows {
            let test_id: String = row.try_get("test_id")?;
            test_ids.push(test_id);
        }

        Ok(test_ids)
    }
}

impl TestIndex for TestIndexImpl {
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
    async fn test_test_index() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("tests.db");

        let index = TestIndexImpl::new(db_path).await.unwrap();

        // Index a test
        index
            .index_test(
                "test_001",
                "test_my_function",
                "tests/test_main.rs",
                "cargo",
                &["my_function".to_string(), "helper".to_string()],
                "test-repo",
                "abc123",
            )
            .await
            .unwrap();

        // Search for tests
        let results = index.search_async("my_function", 10).await.unwrap();
        assert!(!results.is_empty());

        // Get tests for symbol
        let test_ids = index.get_tests_for_symbol("my_function").await.unwrap();
        assert_eq!(test_ids.len(), 1);
        assert_eq!(test_ids[0], "test_001");
    }
}
