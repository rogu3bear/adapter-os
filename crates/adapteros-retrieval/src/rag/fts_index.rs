//! Full-text search index implementations using SQLite FTS5
//!
//! Provides high-performance FTS5-based indices for symbols, tests, and documentation
//! with per-tenant isolation and deterministic ordering.

use crate::codegraph::types::SymbolNode;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;

/// Symbol stored in FTS5 index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedSymbol {
    pub symbol_id: String,
    pub name: String,
    pub file_path: String,
    pub start_line: i32,
    pub end_line: i32,
    pub kind: String,
    pub signature: Option<String>,
    pub visibility: String,
    pub repo_id: String,
    pub commit_sha: String,
    pub docstring: Option<String>,
    pub module_path: String,
}

/// Test case stored in FTS5 index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTest {
    pub test_id: String,
    pub test_name: String,
    pub file_path: String,
    pub start_line: i32,
    pub end_line: i32,
    pub target_symbol_id: Option<String>,
    pub target_function: Option<String>,
    pub repo_id: String,
    pub commit_sha: String,
}

/// Documentation entry stored in FTS5 index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedDoc {
    pub doc_id: String,
    pub doc_type: String, // README, ADR, doc_comment, etc.
    pub file_path: String,
    pub title: String,
    pub content: String,
    pub repo_id: String,
    pub commit_sha: String,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
}

/// Symbol index implementation using SQLite FTS5
pub struct SymbolIndexImpl {
    pool: SqlitePool,
    tenant_id: String,
}

impl SymbolIndexImpl {
    /// Create a new symbol index for a tenant
    pub async fn new(index_path: PathBuf, tenant_id: String) -> Result<Self> {
        // Ensure directory exists
        tokio::fs::create_dir_all(&index_path).await?;

        let db_path = index_path.join("symbols.db");
        let connection_string = format!("sqlite://{}?mode=rwc", db_path.display());

        let pool = SqlitePool::connect(&connection_string)
            .await
            .context("Failed to connect to symbols database")?;

        // Create FTS5 table for full-text search with tenant isolation
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
                symbol_id UNINDEXED,
                name,
                file_path UNINDEXED,
                start_line UNINDEXED,
                end_line UNINDEXED,
                kind,
                signature,
                visibility,
                repo_id UNINDEXED,
                commit_sha UNINDEXED,
                docstring,
                module_path,
                tenant_id UNINDEXED,
                tokenize = 'porter unicode61'
            );
            "#,
        )
        .execute(&pool)
        .await?;

        // Create metadata table for tracking
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS symbols_metadata (
                symbol_id TEXT PRIMARY KEY,
                repo_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                commit_sha TEXT NOT NULL,
                last_updated TEXT NOT NULL DEFAULT (datetime('now')),
                file_hash TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_symbols_metadata_repo ON symbols_metadata(repo_id);
            CREATE INDEX IF NOT EXISTS idx_symbols_metadata_file ON symbols_metadata(file_path);
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool, tenant_id })
    }

    /// Index symbols from a parsed file
    pub async fn index_symbols(
        &self,
        symbols: Vec<SymbolNode>,
        repo_id: &str,
        commit_sha: &str,
        file_hash: &str,
    ) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let mut count = 0;

        for symbol in symbols {
            let symbol_id = symbol.id.to_hex();
            let module_path = symbol.module_path.join("::");

            // Insert into FTS5 table with tenant isolation
            sqlx::query(
                r#"
                INSERT INTO symbols_fts (
                    symbol_id, name, file_path, start_line, end_line,
                    kind, signature, visibility, repo_id, commit_sha,
                    docstring, module_path, tenant_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&symbol_id)
            .bind(&symbol.name)
            .bind(&symbol.file_path)
            .bind(symbol.span.start_line as i32)
            .bind(symbol.span.end_line as i32)
            .bind(symbol.kind.to_string())
            .bind(symbol.signature)
            .bind(symbol.visibility.to_string())
            .bind(repo_id)
            .bind(commit_sha)
            .bind(symbol.docstring)
            .bind(&module_path)
            .bind(&self.tenant_id)
            .execute(&mut *tx)
            .await?;

            // Insert metadata
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO symbols_metadata (
                    symbol_id, repo_id, file_path, commit_sha, file_hash
                ) VALUES (?, ?, ?, ?, ?)
                "#,
            )
            .bind(&symbol_id)
            .bind(repo_id)
            .bind(&symbol.file_path)
            .bind(commit_sha)
            .bind(file_hash)
            .execute(&mut *tx)
            .await?;

            count += 1;
        }

        tx.commit().await?;
        Ok(count)
    }

    /// Search symbols by query with tenant isolation
    pub async fn search(
        &self,
        query: &str,
        repo_id: Option<&str>,
        max_results: usize,
    ) -> Result<Vec<IndexedSymbol>> {
        // Always filter by tenant_id for isolation
        let sql = if let Some(_repo_id) = repo_id {
            format!(
                r#"
                SELECT * FROM symbols_fts
                WHERE symbols_fts MATCH ? AND repo_id = ? AND tenant_id = ?
                ORDER BY rank
                LIMIT {}
                "#,
                max_results
            )
        } else {
            format!(
                r#"
                SELECT * FROM symbols_fts
                WHERE symbols_fts MATCH ? AND tenant_id = ?
                ORDER BY rank
                LIMIT {}
                "#,
                max_results
            )
        };

        let mut query_builder = sqlx::query(&sql).bind(query);
        if let Some(repo_id) = repo_id {
            query_builder = query_builder.bind(repo_id).bind(&self.tenant_id);
        } else {
            query_builder = query_builder.bind(&self.tenant_id);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let symbols = rows
            .into_iter()
            .map(|row| {
                Ok(IndexedSymbol {
                    symbol_id: row.try_get("symbol_id")?,
                    name: row.try_get("name")?,
                    file_path: row.try_get("file_path")?,
                    start_line: row.try_get("start_line")?,
                    end_line: row.try_get("end_line")?,
                    kind: row.try_get("kind")?,
                    signature: row.try_get("signature")?,
                    visibility: row.try_get("visibility")?,
                    repo_id: row.try_get("repo_id")?,
                    commit_sha: row.try_get("commit_sha")?,
                    docstring: row.try_get("docstring")?,
                    module_path: row.try_get("module_path")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(symbols)
    }

    /// Remove symbols for a specific file
    pub async fn remove_file_symbols(&self, file_path: &str, repo_id: &str) -> Result<usize> {
        let result = sqlx::query(
            r#"
            DELETE FROM symbols_fts
            WHERE file_path = ? AND repo_id = ?
            "#,
        )
        .bind(file_path)
        .bind(repo_id)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM symbols_metadata
            WHERE file_path = ? AND repo_id = ?
            "#,
        )
        .bind(file_path)
        .bind(repo_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    /// Get total symbol count
    pub async fn count(&self) -> Result<usize> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM symbols_metadata")
            .fetch_one(&self.pool)
            .await?;
        let count: i64 = row.try_get("count")?;
        Ok(count as usize)
    }
}

/// Test index implementation using SQLite FTS5
pub struct TestIndexImpl {
    pool: SqlitePool,
    tenant_id: String,
}

impl TestIndexImpl {
    /// Create a new test index for a tenant
    pub async fn new(index_path: PathBuf, tenant_id: String) -> Result<Self> {
        // Ensure directory exists
        tokio::fs::create_dir_all(&index_path).await?;

        let db_path = index_path.join("tests.db");
        let connection_string = format!("sqlite://{}?mode=rwc", db_path.display());

        let pool = SqlitePool::connect(&connection_string)
            .await
            .context("Failed to connect to tests database")?;

        // Create FTS5 table with tenant isolation
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS tests_fts USING fts5(
                test_id UNINDEXED,
                test_name,
                file_path UNINDEXED,
                start_line UNINDEXED,
                end_line UNINDEXED,
                target_symbol_id UNINDEXED,
                target_function,
                repo_id UNINDEXED,
                commit_sha UNINDEXED,
                tenant_id UNINDEXED,
                tokenize = 'porter unicode61'
            );
            "#,
        )
        .execute(&pool)
        .await?;

        // Create metadata table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tests_metadata (
                test_id TEXT PRIMARY KEY,
                repo_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                commit_sha TEXT NOT NULL,
                last_updated TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_tests_metadata_repo ON tests_metadata(repo_id);
            CREATE INDEX IF NOT EXISTS idx_tests_metadata_file ON tests_metadata(file_path);
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool, tenant_id })
    }

    /// Index tests from a file
    pub async fn index_tests(
        &self,
        tests: Vec<IndexedTest>,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let mut count = 0;

        for test in tests {
            // Insert into FTS5 table with tenant isolation
            sqlx::query(
                r#"
                INSERT INTO tests_fts (
                    test_id, test_name, file_path, start_line, end_line,
                    target_symbol_id, target_function, repo_id, commit_sha, tenant_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&test.test_id)
            .bind(&test.test_name)
            .bind(&test.file_path)
            .bind(test.start_line)
            .bind(test.end_line)
            .bind(&test.target_symbol_id)
            .bind(&test.target_function)
            .bind(repo_id)
            .bind(commit_sha)
            .bind(&self.tenant_id)
            .execute(&mut *tx)
            .await?;

            // Insert metadata
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO tests_metadata (
                    test_id, repo_id, file_path, commit_sha
                ) VALUES (?, ?, ?, ?)
                "#,
            )
            .bind(&test.test_id)
            .bind(repo_id)
            .bind(&test.file_path)
            .bind(commit_sha)
            .execute(&mut *tx)
            .await?;

            count += 1;
        }

        tx.commit().await?;
        Ok(count)
    }

    /// Search tests by query with tenant isolation
    pub async fn search(
        &self,
        query: &str,
        repo_id: Option<&str>,
        max_results: usize,
    ) -> Result<Vec<IndexedTest>> {
        // Always filter by tenant_id for isolation
        let sql = if let Some(_repo_id) = repo_id {
            format!(
                r#"
                SELECT * FROM tests_fts
                WHERE tests_fts MATCH ? AND repo_id = ? AND tenant_id = ?
                ORDER BY rank
                LIMIT {}
                "#,
                max_results
            )
        } else {
            format!(
                r#"
                SELECT * FROM tests_fts
                WHERE tests_fts MATCH ? AND tenant_id = ?
                ORDER BY rank
                LIMIT {}
                "#,
                max_results
            )
        };

        let mut query_builder = sqlx::query(&sql).bind(query);
        if let Some(repo_id) = repo_id {
            query_builder = query_builder.bind(repo_id).bind(&self.tenant_id);
        } else {
            query_builder = query_builder.bind(&self.tenant_id);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let tests = rows
            .into_iter()
            .map(|row| {
                Ok(IndexedTest {
                    test_id: row.try_get("test_id")?,
                    test_name: row.try_get("test_name")?,
                    file_path: row.try_get("file_path")?,
                    start_line: row.try_get("start_line")?,
                    end_line: row.try_get("end_line")?,
                    target_symbol_id: row.try_get("target_symbol_id")?,
                    target_function: row.try_get("target_function")?,
                    repo_id: row.try_get("repo_id")?,
                    commit_sha: row.try_get("commit_sha")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(tests)
    }

    /// Remove tests for a specific file
    pub async fn remove_file_tests(&self, file_path: &str, repo_id: &str) -> Result<usize> {
        let result = sqlx::query(
            r#"
            DELETE FROM tests_fts
            WHERE file_path = ? AND repo_id = ?
            "#,
        )
        .bind(file_path)
        .bind(repo_id)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM tests_metadata
            WHERE file_path = ? AND repo_id = ?
            "#,
        )
        .bind(file_path)
        .bind(repo_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    /// Get total test count
    pub async fn count(&self) -> Result<usize> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM tests_metadata")
            .fetch_one(&self.pool)
            .await?;
        let count: i64 = row.try_get("count")?;
        Ok(count as usize)
    }
}

/// Documentation index implementation using SQLite FTS5
pub struct DocIndexImpl {
    pool: SqlitePool,
    tenant_id: String,
}

impl DocIndexImpl {
    /// Create a new doc index for a tenant
    pub async fn new(index_path: PathBuf, tenant_id: String) -> Result<Self> {
        // Ensure directory exists
        tokio::fs::create_dir_all(&index_path).await?;

        let db_path = index_path.join("docs.db");
        let connection_string = format!("sqlite://{}?mode=rwc", db_path.display());

        let pool = SqlitePool::connect(&connection_string)
            .await
            .context("Failed to connect to docs database")?;

        // Create FTS5 table with tenant isolation
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
                doc_id UNINDEXED,
                doc_type,
                file_path UNINDEXED,
                title,
                content,
                repo_id UNINDEXED,
                commit_sha UNINDEXED,
                start_line UNINDEXED,
                end_line UNINDEXED,
                tenant_id UNINDEXED,
                tokenize = 'porter unicode61'
            );
            "#,
        )
        .execute(&pool)
        .await?;

        // Create metadata table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS docs_metadata (
                doc_id TEXT PRIMARY KEY,
                repo_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                commit_sha TEXT NOT NULL,
                last_updated TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_docs_metadata_repo ON docs_metadata(repo_id);
            CREATE INDEX IF NOT EXISTS idx_docs_metadata_file ON docs_metadata(file_path);
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool, tenant_id })
    }

    /// Index documentation entries
    pub async fn index_docs(
        &self,
        docs: Vec<IndexedDoc>,
        repo_id: &str,
        commit_sha: &str,
    ) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let mut count = 0;

        for doc in docs {
            // Insert into FTS5 table with tenant isolation
            sqlx::query(
                r#"
                INSERT INTO docs_fts (
                    doc_id, doc_type, file_path, title, content,
                    repo_id, commit_sha, start_line, end_line, tenant_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&doc.doc_id)
            .bind(&doc.doc_type)
            .bind(&doc.file_path)
            .bind(&doc.title)
            .bind(&doc.content)
            .bind(repo_id)
            .bind(commit_sha)
            .bind(doc.start_line)
            .bind(doc.end_line)
            .bind(&self.tenant_id)
            .execute(&mut *tx)
            .await?;

            // Insert metadata
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO docs_metadata (
                    doc_id, repo_id, file_path, commit_sha
                ) VALUES (?, ?, ?, ?)
                "#,
            )
            .bind(&doc.doc_id)
            .bind(repo_id)
            .bind(&doc.file_path)
            .bind(commit_sha)
            .execute(&mut *tx)
            .await?;

            count += 1;
        }

        tx.commit().await?;
        Ok(count)
    }

    /// Search documentation by query with tenant isolation
    pub async fn search(
        &self,
        query: &str,
        repo_id: Option<&str>,
        max_results: usize,
    ) -> Result<Vec<IndexedDoc>> {
        // Always filter by tenant_id for isolation
        let sql = if let Some(_repo_id) = repo_id {
            format!(
                r#"
                SELECT * FROM docs_fts
                WHERE docs_fts MATCH ? AND repo_id = ? AND tenant_id = ?
                ORDER BY rank
                LIMIT {}
                "#,
                max_results
            )
        } else {
            format!(
                r#"
                SELECT * FROM docs_fts
                WHERE docs_fts MATCH ? AND tenant_id = ?
                ORDER BY rank
                LIMIT {}
                "#,
                max_results
            )
        };

        let mut query_builder = sqlx::query(&sql).bind(query);
        if let Some(repo_id) = repo_id {
            query_builder = query_builder.bind(repo_id).bind(&self.tenant_id);
        } else {
            query_builder = query_builder.bind(&self.tenant_id);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let docs = rows
            .into_iter()
            .map(|row| {
                Ok(IndexedDoc {
                    doc_id: row.try_get("doc_id")?,
                    doc_type: row.try_get("doc_type")?,
                    file_path: row.try_get("file_path")?,
                    title: row.try_get("title")?,
                    content: row.try_get("content")?,
                    repo_id: row.try_get("repo_id")?,
                    commit_sha: row.try_get("commit_sha")?,
                    start_line: row.try_get("start_line")?,
                    end_line: row.try_get("end_line")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(docs)
    }

    /// Remove docs for a specific file
    pub async fn remove_file_docs(&self, file_path: &str, repo_id: &str) -> Result<usize> {
        let result = sqlx::query(
            r#"
            DELETE FROM docs_fts
            WHERE file_path = ? AND repo_id = ?
            "#,
        )
        .bind(file_path)
        .bind(repo_id)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM docs_metadata
            WHERE file_path = ? AND repo_id = ?
            "#,
        )
        .bind(file_path)
        .bind(repo_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    /// Get total doc count
    pub async fn count(&self) -> Result<usize> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM docs_metadata")
            .fetch_one(&self.pool)
            .await?;
        let count: i64 = row.try_get("count")?;
        Ok(count as usize)
    }
}
