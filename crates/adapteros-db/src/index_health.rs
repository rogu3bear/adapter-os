//! SQLite index health and maintenance helpers.
//!
//! This module provides lightweight, read-only introspection for:
//! - Database fragmentation (freelist ratio)
//! - Tenant index coverage checks (tenant_id leading indexes)
//! - Optional dbstat-based index btree fragmentation (unused bytes ratio)

use crate::Db;
use adapteros_core::{AosError, Result};
use sqlx::Row;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SqlitePageStats {
    pub page_size_bytes: u64,
    pub page_count: u64,
    pub freelist_count: u64,
    pub db_size_estimate_bytes: u64,
    pub freelist_bytes: u64,
    pub freelist_ratio: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TenantIndexCoverage {
    pub table: String,
    pub table_exists: bool,
    pub has_tenant_id_column: bool,
    pub has_leading_tenant_id_index: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbstatIndexObject {
    pub name: String,
    pub bytes: u64,
    pub unused_ratio: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbstatIndexSummary {
    pub total_index_bytes: u64,
    pub total_index_unused_bytes: u64,
    pub total_index_unused_ratio: f64,
    pub top_indexes: Vec<DbstatIndexObject>,
}

fn is_valid_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

impl Db {
    /// Collect SQLite page and freelist stats (global fragmentation signal).
    ///
    /// Returns `Ok(None)` when the current storage mode has no SQL pool attached.
    pub async fn collect_sqlite_page_stats(&self) -> Result<Option<SqlitePageStats>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(None);
        };

        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read PRAGMA page_size: {}", e)))?;
        let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
            .fetch_one(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read PRAGMA page_count: {}", e)))?;
        let freelist_count: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(pool)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to read PRAGMA freelist_count: {}", e))
            })?;

        let page_size_bytes = page_size.max(0) as u64;
        let page_count_u64 = page_count.max(0) as u64;
        let freelist_count_u64 = freelist_count.max(0) as u64;

        let db_size_estimate_bytes = page_size_bytes.saturating_mul(page_count_u64);
        let freelist_bytes = page_size_bytes.saturating_mul(freelist_count_u64);
        let freelist_ratio = if page_count_u64 == 0 {
            0.0
        } else {
            freelist_count_u64 as f64 / page_count_u64 as f64
        };

        Ok(Some(SqlitePageStats {
            page_size_bytes,
            page_count: page_count_u64,
            freelist_count: freelist_count_u64,
            db_size_estimate_bytes,
            freelist_bytes,
            freelist_ratio,
        }))
    }

    /// Verify tenant-scoped index coverage for a set of tables.
    ///
    /// The check is intentionally conservative: it reports coverage only when there
    /// exists an index whose first column is `tenant_id`.
    ///
    /// Returns an empty vector when the current storage mode has no SQL pool attached.
    pub async fn collect_tenant_index_coverage(
        &self,
        tables: &[&str],
    ) -> Result<Vec<TenantIndexCoverage>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(Vec::new());
        };

        let mut results = Vec::with_capacity(tables.len());

        for &table in tables {
            if !is_valid_identifier(table) {
                return Err(AosError::Validation(format!(
                    "Invalid table identifier for coverage check: {}",
                    table
                ))
                .into());
            }

            let table_ident = quote_ident(table);

            let table_info_rows = match sqlx::query(&format!("PRAGMA table_info({})", table_ident))
                .fetch_all(pool)
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("no such table") {
                        results.push(TenantIndexCoverage {
                            table: table.to_string(),
                            table_exists: false,
                            has_tenant_id_column: false,
                            has_leading_tenant_id_index: false,
                        });
                        continue;
                    }
                    return Err(AosError::Database(format!(
                        "Failed to read PRAGMA table_info({}): {}",
                        table, e
                    ))
                    .into());
                }
            };

            let has_tenant_id_column = table_info_rows
                .iter()
                .any(|row| row.get::<String, _>("name") == "tenant_id");

            let mut has_leading_tenant_id_index = false;

            if has_tenant_id_column {
                let index_list_rows = sqlx::query(&format!("PRAGMA index_list({})", table_ident))
                    .fetch_all(pool)
                    .await
                    .map_err(|e| {
                        AosError::Database(format!(
                            "Failed to read PRAGMA index_list({}): {}",
                            table, e
                        ))
                    })?;

                for idx_row in index_list_rows {
                    let index_name: String = idx_row.get("name");
                    let index_ident = quote_ident(&index_name);
                    let index_info_rows =
                        sqlx::query(&format!("PRAGMA index_info({})", index_ident))
                            .fetch_all(pool)
                            .await
                            .map_err(|e| {
                                AosError::Database(format!(
                                    "Failed to read PRAGMA index_info({}): {}",
                                    index_name, e
                                ))
                            })?;

                    let Some(first) = index_info_rows
                        .iter()
                        .min_by_key(|r| r.get::<i64, _>("seqno"))
                    else {
                        continue;
                    };
                    let col_name: String = first.get("name");
                    if col_name == "tenant_id" {
                        has_leading_tenant_id_index = true;
                        break;
                    }
                }
            }

            results.push(TenantIndexCoverage {
                table: table.to_string(),
                table_exists: true,
                has_tenant_id_column,
                has_leading_tenant_id_index,
            });
        }

        Ok(results)
    }

    /// Collect dbstat-based index btree fragmentation metrics (unused bytes ratio).
    ///
    /// Returns `Ok(None)` if the current SQLite build does not support the `dbstat`
    /// virtual table (missing `SQLITE_ENABLE_DBSTAT_VTAB`) or when no SQL pool is attached.
    pub async fn collect_dbstat_index_summary(
        &self,
        top_n: usize,
    ) -> Result<Option<DbstatIndexSummary>> {
        let Some(pool) = self.pool_opt() else {
            return Ok(None);
        };

        let rows = match sqlx::query(
            r#"
            SELECT
              d.name AS name,
              SUM(d.pgsize) AS bytes,
              SUM(d.unused) AS unused_bytes
            FROM dbstat d
            LEFT JOIN sqlite_master m ON m.name = d.name
            WHERE m.type = 'index' OR d.name LIKE 'sqlite_autoindex_%'
            GROUP BY d.name
            ORDER BY bytes DESC
            "#,
        )
        .fetch_all(pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("no such table: dbstat") {
                    return Ok(None);
                }
                return Err(AosError::Database(format!(
                    "Failed to query dbstat index summary: {}",
                    e
                ))
                .into());
            }
        };

        let mut total_index_bytes: u64 = 0;
        let mut total_index_unused_bytes: u64 = 0;
        let mut top_indexes: Vec<DbstatIndexObject> = Vec::new();

        for (idx, row) in rows.into_iter().enumerate() {
            let name: String = row.get("name");
            let bytes: i64 = row.get("bytes");
            let unused_bytes: i64 = row.get("unused_bytes");

            let bytes_u64 = bytes.max(0) as u64;
            let unused_u64 = unused_bytes.max(0) as u64;

            total_index_bytes = total_index_bytes.saturating_add(bytes_u64);
            total_index_unused_bytes = total_index_unused_bytes.saturating_add(unused_u64);

            if idx < top_n && bytes_u64 > 0 {
                let unused_ratio = unused_u64 as f64 / bytes_u64 as f64;
                top_indexes.push(DbstatIndexObject {
                    name,
                    bytes: bytes_u64,
                    unused_ratio,
                });
            }
        }

        let total_index_unused_ratio = if total_index_bytes == 0 {
            0.0
        } else {
            total_index_unused_bytes as f64 / total_index_bytes as f64
        };

        Ok(Some(DbstatIndexSummary {
            total_index_bytes,
            total_index_unused_bytes,
            total_index_unused_ratio,
            top_indexes,
        }))
    }
}
