//! aos-secd audit command

use adapteros_db::Db;
use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use std::path::Path;

/// Display aos-secd operation audit trail
pub async fn run(db_path: &Path, limit: i64, operation_type: Option<&str>) -> Result<()> {
    let db = Db::connect(
        db_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
    )
    .await?;

    println!("aos-secd Audit Trail");
    println!("════════════════════");
    println!();

    // Get operations
    let operations = if let Some(op_type) = operation_type {
        db.list_enclave_operations_by_type(op_type, limit).await?
    } else {
        db.list_enclave_operations(limit).await?
    };

    if operations.is_empty() {
        println!("No operations logged yet.");
        return Ok(());
    }

    // Create table
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Add header
    table.set_header(vec![
        Cell::new("Timestamp").fg(Color::Cyan),
        Cell::new("Operation").fg(Color::Cyan),
        Cell::new("Artifact Hash").fg(Color::Cyan),
        Cell::new("Result").fg(Color::Cyan),
        Cell::new("Error").fg(Color::Cyan),
    ]);

    // Add rows
    for op in &operations {
        let timestamp = format_timestamp(op.timestamp);
        let hash_display = op
            .artifact_hash
            .as_ref()
            .map(|h| truncate_hash(h))
            .unwrap_or_else(|| "-".to_string());

        let result_cell = if op.result == "success" {
            Cell::new(&op.result).fg(Color::Green)
        } else {
            Cell::new(&op.result).fg(Color::Red)
        };

        let error_display = op
            .error_message
            .as_ref()
            .map(|e| truncate_error(e))
            .unwrap_or_else(|| "-".to_string());

        table.add_row(vec![
            Cell::new(timestamp),
            Cell::new(&op.operation),
            Cell::new(hash_display),
            result_cell,
            Cell::new(error_display),
        ]);
    }

    println!("{table}");
    println!();
    println!("Showing {} most recent operations", operations.len());

    // Print statistics
    let stats = db.get_operation_stats().await?;
    if !stats.is_empty() {
        println!();
        println!("Operation Statistics:");
        println!("────────────────────");

        for stat in stats {
            let success_rate = if stat.count > 0 {
                (stat.success_count as f64 / stat.count as f64) * 100.0
            } else {
                0.0
            };

            println!(
                "  {:<15} {:>6} total  ({:>6} success, {:>6} error)  {:.1}% success rate",
                stat.operation, stat.count, stat.success_count, stat.error_count, success_rate
            );
        }
    }

    Ok(())
}

fn format_timestamp(timestamp: i64) -> String {
    use chrono::{DateTime, Local, TimeZone};

    let dt = Local
        .timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or_else(|| Local::now());
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn truncate_hash(hash: &str) -> String {
    if hash.len() > 12 {
        format!("{}...", &hash[..12])
    } else {
        hash.to_string()
    }
}

fn truncate_error(error: &str) -> String {
    if error.len() > 40 {
        format!("{}...", &error[..37])
    } else {
        error.to_string()
    }
}
