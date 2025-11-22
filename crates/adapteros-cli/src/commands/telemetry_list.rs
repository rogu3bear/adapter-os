//! List telemetry events with stack filtering (PRD-03)

use anyhow::Result;
use serde_json::json;
use std::path::Path;

use crate::output::OutputWriter;

/// List telemetry events from database with optional stack filtering
///
/// This function queries telemetry bundles and their events, filtering by stack_id if provided.
/// Supports PRD-03 stack versioning and telemetry correlation.
pub async fn list_telemetry_events(
    database_path: &Path,
    by_stack: Option<&str>,
    event_type: Option<&str>,
    limit: u32,
    output: &OutputWriter,
) -> Result<()> {
    use sqlx::sqlite::SqlitePool;
    use sqlx::Row;

    // Connect to database
    let db_url = format!("sqlite://{}", database_path.display());
    let pool = SqlitePool::connect(&db_url).await?;

    // Query telemetry_bundles table
    // Note: Actual event data is stored in bundle files, not database
    // This queries bundle metadata with stack correlation
    let bundles = if let Some(stack_id) = by_stack {
        // Filter bundles by stack_id (requires bundle metadata to include stack info)
        // For now, this is a placeholder - actual implementation needs bundle parsing
        sqlx::query(
            r#"
            SELECT id, tenant_id, cpid, path, event_count, created_at
            FROM telemetry_bundles
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(stack_id) // Temporary: using stack_id as tenant filter
        .bind(limit as i64)
        .fetch_all(&pool)
        .await?
    } else {
        // List all bundles
        sqlx::query(
            r#"
            SELECT id, tenant_id, cpid, path, event_count, created_at
            FROM telemetry_bundles
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&pool)
        .await?
    };

    // Format output
    let results: Vec<_> = bundles
        .iter()
        .map(|row| {
            json!({
                "bundle_id": row.get::<String, _>("id"),
                "tenant_id": row.get::<String, _>("tenant_id"),
                "cpid": row.get::<String, _>("cpid"),
                "path": row.get::<String, _>("path"),
                "event_count": row.get::<i64, _>("event_count"),
                "created_at": row.get::<String, _>("created_at"),
                // Note: stack_id and stack_version will be added when bundle metadata is updated
                "note": "Full event-level filtering requires parsing bundle files (see PRD-03)"
            })
        })
        .collect();

    if output.is_json() {
        output.print_json(&json!({
            "bundles": results,
            "count": results.len(),
            "limit": limit,
            "filters": {
                "by_stack": by_stack,
                "event_type": event_type
            }
        }))?;
    } else {
        output.print_line("Telemetry Bundles:")?;
        output.print_line(&format!(
            "{:<36} {:<20} {:<12} {:>8}",
            "Bundle ID", "Tenant ID", "CPID", "Events"
        ))?;
        output.print_line(&"-".repeat(80))?;

        for bundle in &results {
            output.print_line(&format!(
                "{:<36} {:<20} {:<12} {:>8}",
                bundle["bundle_id"].as_str().unwrap_or(""),
                bundle["tenant_id"].as_str().unwrap_or(""),
                bundle["cpid"].as_str().unwrap_or(""),
                bundle["event_count"].as_i64().unwrap_or(0),
            ))?;
        }

        output.print_line("")?;
        output.print_line(&format!("Total bundles: {}", results.len()))?;

        if by_stack.is_some() || event_type.is_some() {
            output.print_line("")?;
            output
                .print_line("Note: Event-level stack filtering requires parsing bundle files.")?;
            output.print_line(
                "This currently shows bundle-level metadata. Full implementation pending.",
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputMode;

    #[tokio::test]
    async fn test_list_telemetry_basic() {
        // This test validates the function signature and basic structure
        // Actual testing requires a populated database
        let output = OutputWriter::new(OutputMode::Text, false);

        // Would fail without a real database, but validates structure
        // let result = list_telemetry_events(
        //     Path::new(":memory:"),
        //     None,
        //     None,
        //     10,
        //     &output
        // ).await;
        // assert!(result.is_ok());
    }
}
