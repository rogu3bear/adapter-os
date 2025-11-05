//! CodeGraph database statistics
//!
//! Provides statistics and information about CodeGraph databases,
//! including symbol counts, edge counts, and database health metrics.

use crate::output::OutputWriter;
use adapteros_codegraph::sqlite::CodeGraphDb;
use adapteros_core::Result;

/// Run the codegraph stats command
pub async fn run(codegraph_db: std::path::PathBuf, output: &OutputWriter) -> Result<()> {
    output.info(format!(
        "Analyzing CodeGraph database: {}",
        codegraph_db.display()
    ));

    // Check if database file exists
    if !codegraph_db.exists() {
        output.error(format!(
            "Database file does not exist: {}",
            codegraph_db.display()
        ));
        return Ok(());
    }

    // Check file permissions and readability
    match std::fs::metadata(&codegraph_db) {
        Ok(metadata) => {
            if !metadata.is_file() {
                output.error(format!("Path is not a file: {}", codegraph_db.display()));
                return Ok(());
            }

            // Check if file is readable
            match std::fs::File::open(&codegraph_db) {
                Ok(_) => {}
                Err(e) => {
                    output.error(format!(
                        "Cannot read database file: {} (error: {})",
                        codegraph_db.display(),
                        e
                    ));
                    return Ok(());
                }
            }

            output.kv("Database Size", &format_file_size(metadata.len()));
        }
        Err(e) => {
            output.error(format!(
                "Cannot access database file: {} (error: {})",
                codegraph_db.display(),
                e
            ));
            return Ok(());
        }
    }

    // Open database connection with error handling
    let db = match CodeGraphDb::new(&codegraph_db).await {
        Ok(db) => {
            output.success("Connected to CodeGraph database");
            db
        }
        Err(e) => {
            output.error(format!("Failed to open database: {}", e));
            output.info("The database file may be corrupted or from an incompatible version");
            return Ok(());
        }
    };

    // Get database statistics with error handling
    let stats = match db.get_stats().await {
        Ok(stats) => stats,
        Err(e) => {
            output.error(format!("Failed to read database statistics: {}", e));
            output.info("The database may be corrupted or incomplete");
            return Ok(());
        }
    };

    // Display statistics
    output.blank();
    output.section("CodeGraph Database Statistics");

    output.kv("Database Path", &codegraph_db.display().to_string());
    output.blank();

    output.kv("Symbols", &stats.symbol_count.to_string());
    output.kv("Call Edges", &stats.edge_count.to_string());
    output.kv("Import Edges", &stats.import_edge_count.to_string());
    output.kv(
        "Total Edges",
        &(stats.edge_count + stats.import_edge_count).to_string(),
    );

    // Calculate some derived metrics
    let total_entities = stats.symbol_count + stats.edge_count + stats.import_edge_count;
    output.kv("Total Entities", &total_entities.to_string());

    if stats.symbol_count > 0 {
        let avg_edges_per_symbol =
            (stats.edge_count + stats.import_edge_count) as f64 / stats.symbol_count as f64;
        output.kv(
            "Avg Edges per Symbol",
            &format!("{:.2}", avg_edges_per_symbol),
        );
    } else {
        output.kv("Avg Edges per Symbol", "N/A (no symbols)");
    }

    // Health check
    output.blank();
    output.section("Database Health");

    let mut health_issues = Vec::new();

    if stats.symbol_count == 0 {
        health_issues.push("No symbols found - database may be empty");
    }

    if stats.edge_count == 0 && stats.import_edge_count == 0 {
        health_issues.push("No edges found - missing call graph data");
    }

    if total_entities > 1_000_000 {
        output.kv("Health Status", "Large database (>1M entities)");
    } else if total_entities > 100_000 {
        output.kv("Health Status", "Medium database (>100K entities)");
    } else if total_entities > 0 {
        output.kv("Health Status", "Small database");
    } else {
        health_issues.push("Database appears empty");
        output.kv("Health Status", "Empty database");
    }

    if health_issues.is_empty() && total_entities > 0 {
        output.kv("Health Status", "Healthy");
    } else if !health_issues.is_empty() {
        output.kv("Health Status", "Issues detected");
        for issue in &health_issues {
            output.info(format!("  ⚠️  {}", issue));
        }
    }

    // JSON output if requested
    if output.is_json() {
        let db_size = std::fs::metadata(&codegraph_db)
            .map(|m| m.len())
            .unwrap_or(0);

        let json_stats = serde_json::json!({
            "database_path": codegraph_db.display().to_string(),
            "database_size_bytes": db_size,
            "symbols": stats.symbol_count,
            "call_edges": stats.edge_count,
            "import_edges": stats.import_edge_count,
            "total_edges": stats.edge_count + stats.import_edge_count,
            "total_entities": total_entities,
            "health_issues": health_issues,
            "database_exists": true,
            "database_readable": true,
            "statistics_retrievable": true
        });
        output.json(&json_stats)?;
    }

    if health_issues.is_empty() {
        output.success("Statistics retrieved successfully - database appears healthy");
    } else {
        output.info(format!(
            "Statistics retrieved with {} health issues",
            health_issues.len()
        ));
    }

    Ok(())
}

/// Format file size in human-readable format
fn format_file_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(1023), "1023 B");
        assert_eq!(format_file_size(1024), "1.00 KB");
        assert_eq!(format_file_size(1536), "1.50 KB");
        assert_eq!(format_file_size(1048576), "1.00 MB");
    }
}
