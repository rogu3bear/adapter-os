//! First-class error persistence (ErrorInstance + ErrorBucket).
//!
//! Tenant scoping is mandatory on every query. Callers must pass the effective tenant_id.

use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_id::{IdPrefix, TypedId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ErrorInstanceRow {
    pub id: String,
    pub created_at_unix_ms: i64,
    pub tenant_id: String,
    pub source: String,
    pub error_code: String,
    pub kind: String,
    pub severity: String,
    pub message_user: String,
    pub message_dev: Option<String>,
    pub fingerprint: String,
    pub tags_json: String,
    pub session_id: Option<String>,
    pub request_id: Option<String>,
    pub diag_trace_id: Option<String>,
    pub otel_trace_id: Option<String>,
    pub http_method: Option<String>,
    pub http_path: Option<String>,
    pub http_status: Option<i32>,
    pub run_id: Option<String>,
    pub receipt_hash: Option<String>,
    pub route_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ErrorBucketRow {
    pub fingerprint: String,
    pub tenant_id: String,
    pub error_code: String,
    pub kind: String,
    pub severity: String,
    pub first_seen_unix_ms: i64,
    pub last_seen_unix_ms: i64,
    pub count: i64,
    pub sample_error_ids_json: String,
}

#[derive(Debug, Clone, Default)]
pub struct ListErrorsDbQuery {
    pub tenant_id: String,
    pub since_unix_ms: Option<i64>,
    pub until_unix_ms: Option<i64>,
    pub limit: Option<u32>,
    pub after_created_at_unix_ms: Option<i64>,
    pub error_code: Option<String>,
    pub fingerprint: Option<String>,
    pub request_id: Option<String>,
    pub diag_trace_id: Option<String>,
    pub session_id: Option<String>,
    pub source: Option<String>,
    pub severity: Option<String>,
    pub kind: Option<String>,
}

fn severity_rank(sev: &str) -> i32 {
    match sev {
        "fatal" => 3,
        "error" => 2,
        "warn" => 1,
        _ => 0,
    }
}

impl Db {
    pub async fn insert_error_instance(&self, row: &ErrorInstanceRow) -> Result<String> {
        let id = if row.id.trim().is_empty() {
            TypedId::new(IdPrefix::Err).to_string()
        } else {
            row.id.clone()
        };

        sqlx::query(
            r#"
            INSERT INTO error_instances (
                id, created_at_unix_ms, tenant_id, source, error_code, kind, severity,
                message_user, message_dev, fingerprint, tags_json,
                session_id, request_id, diag_trace_id, otel_trace_id,
                http_method, http_path, http_status, run_id, receipt_hash, route_digest
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(row.created_at_unix_ms)
        .bind(&row.tenant_id)
        .bind(&row.source)
        .bind(&row.error_code)
        .bind(&row.kind)
        .bind(&row.severity)
        .bind(&row.message_user)
        .bind(&row.message_dev)
        .bind(&row.fingerprint)
        .bind(&row.tags_json)
        .bind(&row.session_id)
        .bind(&row.request_id)
        .bind(&row.diag_trace_id)
        .bind(&row.otel_trace_id)
        .bind(&row.http_method)
        .bind(&row.http_path)
        .bind(row.http_status)
        .bind(&row.run_id)
        .bind(&row.receipt_hash)
        .bind(&row.route_digest)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("insert_error_instance: {}", e)))?;

        Ok(id)
    }

    pub async fn upsert_error_bucket(
        &self,
        tenant_id: &str,
        fingerprint: &str,
        error_code: &str,
        kind: &str,
        severity: &str,
        seen_unix_ms: i64,
        sample_error_id: &str,
    ) -> Result<()> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(format!("upsert_error_bucket: begin: {}", e)))?;

        let existing = sqlx::query_as::<_, ErrorBucketRow>(
            r#"
            SELECT fingerprint, tenant_id, error_code, kind, severity,
                   first_seen_unix_ms, last_seen_unix_ms, count, sample_error_ids_json
            FROM error_buckets
            WHERE tenant_id = ? AND fingerprint = ?
            "#,
        )
        .bind(tenant_id)
        .bind(fingerprint)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("upsert_error_bucket: select: {}", e)))?;

        if let Some(mut bucket) = existing {
            bucket.last_seen_unix_ms = seen_unix_ms.max(bucket.last_seen_unix_ms);
            bucket.count = bucket.count.saturating_add(1);
            if severity_rank(severity) > severity_rank(&bucket.severity) {
                bucket.severity = severity.to_string();
            }

            let mut sample_ids: Vec<String> =
                serde_json::from_str(&bucket.sample_error_ids_json).unwrap_or_default();
            if sample_ids.len() < 5 && !sample_ids.iter().any(|s| s == sample_error_id) {
                sample_ids.push(sample_error_id.to_string());
            }
            bucket.sample_error_ids_json =
                serde_json::to_string(&sample_ids).unwrap_or("[]".into());

            sqlx::query(
                r#"
                UPDATE error_buckets
                SET last_seen_unix_ms = ?,
                    count = ?,
                    severity = ?,
                    sample_error_ids_json = ?
                WHERE tenant_id = ? AND fingerprint = ?
                "#,
            )
            .bind(bucket.last_seen_unix_ms)
            .bind(bucket.count)
            .bind(&bucket.severity)
            .bind(&bucket.sample_error_ids_json)
            .bind(&bucket.tenant_id)
            .bind(&bucket.fingerprint)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("upsert_error_bucket: update: {}", e)))?;
        } else {
            let sample_json = serde_json::to_string(&vec![sample_error_id.to_string()])
                .unwrap_or_else(|_| "[]".into());
            sqlx::query(
                r#"
                INSERT INTO error_buckets (
                    fingerprint, tenant_id, error_code, kind, severity,
                    first_seen_unix_ms, last_seen_unix_ms, count, sample_error_ids_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(fingerprint)
            .bind(tenant_id)
            .bind(error_code)
            .bind(kind)
            .bind(severity)
            .bind(seen_unix_ms)
            .bind(seen_unix_ms)
            .bind(1i64)
            .bind(sample_json)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("upsert_error_bucket: insert: {}", e)))?;
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("upsert_error_bucket: commit: {}", e)))?;
        Ok(())
    }

    pub async fn get_error_instance(
        &self,
        tenant_id: &str,
        error_id: &str,
    ) -> Result<Option<ErrorInstanceRow>> {
        let row = sqlx::query_as::<_, ErrorInstanceRow>(
            r#"
            SELECT id, created_at_unix_ms, tenant_id, source, error_code, kind, severity,
                   message_user, message_dev, fingerprint, tags_json,
                   session_id, request_id, diag_trace_id, otel_trace_id,
                   http_method, http_path, http_status, run_id, receipt_hash, route_digest
            FROM error_instances
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(error_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("get_error_instance: {}", e)))?;

        Ok(row)
    }

    pub async fn list_error_instances(
        &self,
        q: &ListErrorsDbQuery,
    ) -> Result<Vec<ErrorInstanceRow>> {
        let mut sql = String::from(
            r#"
            SELECT id, created_at_unix_ms, tenant_id, source, error_code, kind, severity,
                   message_user, message_dev, fingerprint, tags_json,
                   session_id, request_id, diag_trace_id, otel_trace_id,
                   http_method, http_path, http_status, run_id, receipt_hash, route_digest
            FROM error_instances
            WHERE tenant_id = ?
            "#,
        );

        if q.since_unix_ms.is_some() {
            sql.push_str(" AND created_at_unix_ms >= ?");
        }
        if q.until_unix_ms.is_some() {
            sql.push_str(" AND created_at_unix_ms <= ?");
        }
        if q.after_created_at_unix_ms.is_some() {
            sql.push_str(" AND created_at_unix_ms < ?");
        }
        if q.error_code.is_some() {
            sql.push_str(" AND error_code = ?");
        }
        if q.fingerprint.is_some() {
            sql.push_str(" AND fingerprint = ?");
        }
        if q.request_id.is_some() {
            sql.push_str(" AND request_id = ?");
        }
        if q.diag_trace_id.is_some() {
            sql.push_str(" AND diag_trace_id = ?");
        }
        if q.session_id.is_some() {
            sql.push_str(" AND session_id = ?");
        }
        if q.source.is_some() {
            sql.push_str(" AND source = ?");
        }
        if q.severity.is_some() {
            sql.push_str(" AND severity = ?");
        }
        if q.kind.is_some() {
            sql.push_str(" AND kind = ?");
        }

        sql.push_str(" ORDER BY created_at_unix_ms DESC");

        let limit = q.limit.unwrap_or(100).min(500) as i64;
        sql.push_str(&format!(" LIMIT {}", limit));

        let mut qb = sqlx::query_as::<_, ErrorInstanceRow>(&sql).bind(&q.tenant_id);
        if let Some(v) = q.since_unix_ms {
            qb = qb.bind(v);
        }
        if let Some(v) = q.until_unix_ms {
            qb = qb.bind(v);
        }
        if let Some(v) = q.after_created_at_unix_ms {
            qb = qb.bind(v);
        }
        if let Some(ref v) = q.error_code {
            qb = qb.bind(v);
        }
        if let Some(ref v) = q.fingerprint {
            qb = qb.bind(v);
        }
        if let Some(ref v) = q.request_id {
            qb = qb.bind(v);
        }
        if let Some(ref v) = q.diag_trace_id {
            qb = qb.bind(v);
        }
        if let Some(ref v) = q.session_id {
            qb = qb.bind(v);
        }
        if let Some(ref v) = q.source {
            qb = qb.bind(v);
        }
        if let Some(ref v) = q.severity {
            qb = qb.bind(v);
        }
        if let Some(ref v) = q.kind {
            qb = qb.bind(v);
        }

        let rows = qb
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("list_error_instances: {}", e)))?;
        Ok(rows)
    }

    pub async fn list_error_buckets(
        &self,
        tenant_id: &str,
        limit: u32,
        error_code: Option<&str>,
    ) -> Result<Vec<ErrorBucketRow>> {
        let mut sql = String::from(
            r#"
            SELECT fingerprint, tenant_id, error_code, kind, severity,
                   first_seen_unix_ms, last_seen_unix_ms, count, sample_error_ids_json
            FROM error_buckets
            WHERE tenant_id = ?
            "#,
        );
        if error_code.is_some() {
            sql.push_str(" AND error_code = ?");
        }
        sql.push_str(" ORDER BY last_seen_unix_ms DESC");
        let limit = (limit as i64).clamp(1, 500);
        sql.push_str(&format!(" LIMIT {}", limit));

        let mut qb = sqlx::query_as::<_, ErrorBucketRow>(&sql).bind(tenant_id);
        if let Some(code) = error_code {
            qb = qb.bind(code);
        }
        let rows = qb
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("list_error_buckets: {}", e)))?;
        Ok(rows)
    }
}
