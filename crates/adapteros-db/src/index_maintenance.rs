//! SQLite index maintenance utilities.
//!
//! These helpers are used by background maintenance automation to keep
//! tenant-scoped operations performant over time.

use crate::Db;
use adapteros_core::{AosError, Result};

fn is_valid_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

impl Db {
    /// Run SQLite PRAGMA optimize (lightweight periodic maintenance).
    pub async fn sqlite_optimize(&self) -> Result<()> {
        if let Some(pool) = self.pool_opt() {
            sqlx::query("PRAGMA optimize")
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("PRAGMA optimize failed: {}", e)))?;
        }
        Ok(())
    }

    /// Run ANALYZE for a curated list of tables.
    ///
    /// When `tables` is empty, runs global `ANALYZE`.
    pub async fn sqlite_analyze_tables(&self, tables: &[&str]) -> Result<()> {
        let Some(pool) = self.pool_opt() else {
            return Ok(());
        };

        if tables.is_empty() {
            sqlx::query("ANALYZE")
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("ANALYZE failed: {}", e)))?;
            return Ok(());
        }

        for &table in tables {
            if !is_valid_identifier(table) {
                return Err(AosError::Validation(format!(
                    "Invalid table identifier for ANALYZE: {}",
                    table
                ))
                .into());
            }
            let table_ident = quote_ident(table);
            sqlx::query(&format!("ANALYZE {}", table_ident))
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("ANALYZE {} failed: {}", table, e)))?;
        }

        Ok(())
    }

    /// Run REINDEX for a curated list of tables.
    ///
    /// When `tables` is empty, runs global `REINDEX`.
    pub async fn sqlite_reindex_tables(&self, tables: &[&str]) -> Result<()> {
        let Some(pool) = self.pool_opt() else {
            return Ok(());
        };

        if tables.is_empty() {
            sqlx::query("REINDEX")
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("REINDEX failed: {}", e)))?;
            return Ok(());
        }

        for &table in tables {
            if !is_valid_identifier(table) {
                return Err(AosError::Validation(format!(
                    "Invalid table identifier for REINDEX: {}",
                    table
                ))
                .into());
            }
            let table_ident = quote_ident(table);
            sqlx::query(&format!("REINDEX {}", table_ident))
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("REINDEX {} failed: {}", table, e)))?;
        }

        Ok(())
    }

    /// Run VACUUM (heavy; requires exclusive lock).
    pub async fn sqlite_vacuum(&self) -> Result<()> {
        let Some(pool) = self.pool_opt() else {
            return Ok(());
        };

        sqlx::query("VACUUM")
            .execute(pool)
            .await
            .map_err(|e| AosError::Database(format!("VACUUM failed: {}", e)))?;
        Ok(())
    }
}
