use std::collections::{HashMap, HashSet};

use adapteros_core::ids::{generate_id, is_readable_id, IdKind};
use anyhow::Result;
use sqlx::{Row, SqlitePool};

use crate::Db;

struct TableSpec {
    table: &'static str,
    kind: IdKind,
    slug_prefs: &'static [&'static str],
    fallback: &'static str,
}

pub async fn backfill_readable_ids(db: &Db) -> Result<()> {
    if !db.storage_mode().read_from_sql() {
        return Ok(());
    }
    let pool = db.pool();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS id_backfill_state (id INTEGER PRIMARY KEY, completed_at TEXT)",
    )
    .execute(pool)
    .await?;

    let completed: Option<String> =
        sqlx::query_scalar("SELECT completed_at FROM id_backfill_state WHERE id = 1")
            .fetch_optional(pool)
            .await?;
    if completed.is_some() {
        return Ok(());
    }

    ensure_alias_table(pool).await?;

    let specs = table_specs();
    let mut table_kinds = HashMap::new();
    for spec in &specs {
        table_kinds.insert(spec.table.to_string(), spec.kind);
    }

    for spec in &specs {
        if !table_has_column(pool, spec.table, "legacy_id").await? {
            continue;
        }
        if !table_has_column(pool, spec.table, "id").await? {
            continue;
        }
        let slug_col = pick_slug_column(pool, spec.table, spec.slug_prefs).await?;
        backfill_table(pool, spec, slug_col.as_deref()).await?;
    }

    // Update FK references based on pragma foreign_key_list
    let tables = list_tables(pool).await?;
    for table in &tables {
        update_foreign_keys(pool, table, &table_kinds).await?;
    }

    sqlx::query("INSERT INTO id_backfill_state (id, completed_at) VALUES (1, datetime('now'))")
        .execute(pool)
        .await?;

    Ok(())
}

async fn ensure_alias_table(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS id_aliases (
            kind TEXT NOT NULL,
            legacy_id TEXT UNIQUE NOT NULL,
            new_id TEXT UNIQUE NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn list_tables(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("name").ok())
        .collect())
}

async fn table_has_column(pool: &SqlitePool, table: &str, column: &str) -> Result<bool> {
    let pragma = format!("PRAGMA table_info({})", table);
    let rows = sqlx::query(&pragma).fetch_all(pool).await?;
    for row in rows {
        let name: String = row.try_get("name")?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn pick_slug_column(
    pool: &SqlitePool,
    table: &str,
    prefs: &[&str],
) -> Result<Option<String>> {
    let pragma = format!("PRAGMA table_info({})", table);
    let rows = sqlx::query(&pragma).fetch_all(pool).await?;
    let mut cols = HashSet::new();
    for row in rows {
        let name: String = row.try_get("name")?;
        cols.insert(name);
    }
    for pref in prefs {
        if cols.contains(*pref) {
            return Ok(Some((*pref).to_string()));
        }
    }
    Ok(None)
}

async fn backfill_table(pool: &SqlitePool, spec: &TableSpec, slug_col: Option<&str>) -> Result<()> {
    let select_sql = if let Some(col) = slug_col {
        format!("SELECT id, legacy_id, {} as slug FROM {}", col, spec.table)
    } else {
        format!("SELECT id, legacy_id, NULL as slug FROM {}", spec.table)
    };
    let rows = sqlx::query(&select_sql).fetch_all(pool).await?;
    for row in rows {
        let id: String = row.try_get("id")?;
        let legacy_id: Option<String> = row.try_get("legacy_id")?;
        if legacy_id.is_some() || is_readable_id(&id) {
            continue;
        }
        let slug: Option<String> = row.try_get("slug")?;
        let slug_source = slug
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(spec.fallback);
        let new_id = generate_id(spec.kind, slug_source);
        sqlx::query("INSERT OR IGNORE INTO id_aliases (kind, legacy_id, new_id) VALUES (?, ?, ?)")
            .bind(spec.kind.prefix())
            .bind(&id)
            .bind(&new_id)
            .execute(pool)
            .await?;
        let update_sql = format!(
            "UPDATE {} SET legacy_id = ?, id = ? WHERE id = ?",
            spec.table
        );
        sqlx::query(&update_sql)
            .bind(&id)
            .bind(&new_id)
            .bind(&id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn update_foreign_keys(
    pool: &SqlitePool,
    table: &str,
    table_kinds: &HashMap<String, IdKind>,
) -> Result<()> {
    let pragma = format!("PRAGMA foreign_key_list({})", table);
    let rows = sqlx::query(&pragma).fetch_all(pool).await?;
    for row in rows {
        let ref_table: String = row.try_get("table")?;
        let from_col: String = row.try_get("from")?;
        if let Some(kind) = table_kinds.get(&ref_table) {
            let update_sql = format!(
                "UPDATE {table} \
                 SET {from_col} = (SELECT new_id FROM id_aliases WHERE legacy_id = {from_col} AND kind = ?) \
                 WHERE {from_col} IN (SELECT legacy_id FROM id_aliases WHERE kind = ?)",
                table = table,
                from_col = from_col
            );
            sqlx::query(&update_sql)
                .bind(kind.prefix())
                .bind(kind.prefix())
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

fn table_specs() -> Vec<TableSpec> {
    vec![
        TableSpec {
            table: "users",
            kind: IdKind::User,
            slug_prefs: &["display_name", "email"],
            fallback: "user",
        },
        TableSpec {
            table: "tenants",
            kind: IdKind::Tenant,
            slug_prefs: &["name", "id"],
            fallback: "tenant",
        },
        TableSpec {
            table: "nodes",
            kind: IdKind::Node,
            slug_prefs: &["hostname"],
            fallback: "node",
        },
        TableSpec {
            table: "models",
            kind: IdKind::Model,
            slug_prefs: &["name"],
            fallback: "model",
        },
        TableSpec {
            table: "adapters",
            kind: IdKind::Adapter,
            slug_prefs: &["name", "adapter_id"],
            fallback: "adapter",
        },
        TableSpec {
            table: "manifests",
            kind: IdKind::Policy,
            slug_prefs: &["hash_b3"],
            fallback: "manifest",
        },
        TableSpec {
            table: "plans",
            kind: IdKind::Plan,
            slug_prefs: &["plan_id_b3"],
            fallback: "plan",
        },
        TableSpec {
            table: "cp_pointers",
            kind: IdKind::Policy,
            slug_prefs: &["name"],
            fallback: "cp",
        },
        TableSpec {
            table: "policies",
            kind: IdKind::Policy,
            slug_prefs: &["hash_b3"],
            fallback: "policy",
        },
        TableSpec {
            table: "jobs",
            kind: IdKind::Job,
            slug_prefs: &["kind"],
            fallback: "job",
        },
        TableSpec {
            table: "telemetry_bundles",
            kind: IdKind::Run,
            slug_prefs: &["cpid"],
            fallback: "bundle",
        },
        TableSpec {
            table: "audits",
            kind: IdKind::Audit,
            slug_prefs: &["suite_name"],
            fallback: "audit",
        },
        TableSpec {
            table: "workers",
            kind: IdKind::Worker,
            slug_prefs: &["node_id"],
            fallback: "worker",
        },
        TableSpec {
            table: "incidents",
            kind: IdKind::Incident,
            slug_prefs: &["kind"],
            fallback: "incident",
        },
        TableSpec {
            table: "training_datasets",
            kind: IdKind::Dataset,
            slug_prefs: &["name"],
            fallback: "dataset",
        },
        TableSpec {
            table: "dataset_files",
            kind: IdKind::File,
            slug_prefs: &["file_name"],
            fallback: "file",
        },
        TableSpec {
            table: "documents",
            kind: IdKind::Document,
            slug_prefs: &["name"],
            fallback: "document",
        },
        TableSpec {
            table: "document_chunks",
            kind: IdKind::Chunk,
            slug_prefs: &["chunk_index"],
            fallback: "chunk",
        },
        TableSpec {
            table: "document_collections",
            kind: IdKind::Collection,
            slug_prefs: &["name"],
            fallback: "collection",
        },
        TableSpec {
            table: "chat_sessions",
            kind: IdKind::Session,
            slug_prefs: &["title", "name"],
            fallback: "chat",
        },
        TableSpec {
            table: "chat_messages",
            kind: IdKind::Message,
            slug_prefs: &["role"],
            fallback: "msg",
        },
        TableSpec {
            table: "adapter_stacks",
            kind: IdKind::Stack,
            slug_prefs: &["name"],
            fallback: "stack",
        },
        TableSpec {
            table: "routing_decisions",
            kind: IdKind::Decision,
            slug_prefs: &["request_id"],
            fallback: "decision",
        },
        TableSpec {
            table: "inference_traces",
            kind: IdKind::Trace,
            slug_prefs: &["request_id"],
            fallback: "trace",
        },
        TableSpec {
            table: "inference_trace_tokens",
            kind: IdKind::Trace,
            slug_prefs: &["trace_id"],
            fallback: "trace",
        },
        TableSpec {
            table: "inference_trace_receipts",
            kind: IdKind::Trace,
            slug_prefs: &["trace_id"],
            fallback: "trace",
        },
        TableSpec {
            table: "replay_executions",
            kind: IdKind::Replay,
            slug_prefs: &["run_id"],
            fallback: "replay",
        },
        TableSpec {
            table: "workspaces",
            kind: IdKind::Workspace,
            slug_prefs: &["name"],
            fallback: "workspace",
        },
        TableSpec {
            table: "workspace_members",
            kind: IdKind::Workspace,
            slug_prefs: &["role"],
            fallback: "member",
        },
        TableSpec {
            table: "workspace_resources",
            kind: IdKind::Workspace,
            slug_prefs: &["resource_type"],
            fallback: "resource",
        },
        TableSpec {
            table: "messages",
            kind: IdKind::Message,
            slug_prefs: &["subject", "thread_id"],
            fallback: "message",
        },
    ]
}
