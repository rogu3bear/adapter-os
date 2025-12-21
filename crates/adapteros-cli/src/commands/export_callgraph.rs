//! Export call graph to various formats
//!
//! Provides CLI command to export CodeGraph call graphs to DOT/Graphviz,
//! JSON, or other formats for analysis and visualization.

use crate::output::OutputWriter;
use adapteros_codegraph::CodeGraph;
use adapteros_core::Result;
use serde::Serialize;
use std::path::Path;

/// Export format options
#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat {
    /// DOT format for Graphviz
    Dot,
    /// JSON format
    Json,
    /// CSV format
    Csv,
}

impl std::str::FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dot" | "graphviz" => Ok(ExportFormat::Dot),
            "json" => Ok(ExportFormat::Json),
            "csv" => Ok(ExportFormat::Csv),
            _ => Err(format!("Unknown export format: {}", s)),
        }
    }
}

#[derive(Serialize)]
struct ExportResult {
    output_path: String,
    format: String,
    symbols: usize,
    edges: usize,
}

/// Export call graph to specified format
pub async fn export_callgraph(
    codegraph_path: &Path,
    output_path: &Path,
    format: ExportFormat,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!(
        "Loading CodeGraph from: {}",
        codegraph_path.display()
    ));

    // Load CodeGraph from database
    let codegraph = CodeGraph::load_from_db(codegraph_path).await?;

    output.kv("Symbols", &codegraph.symbols.len().to_string());
    output.kv("Edges", &codegraph.call_graph.edges.len().to_string());
    output.kv("Content hash", &codegraph.content_hash.to_short_hex());
    output.blank();

    // Export to specified format
    let content = match format {
        ExportFormat::Dot => codegraph.to_dot(),
        ExportFormat::Json => serde_json::to_string_pretty(&codegraph)
            .map_err(|e| adapteros_core::AosError::Serialization(e))?,
        ExportFormat::Csv => export_to_csv(&codegraph),
    };

    // Write to output file
    std::fs::write(output_path, content)
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to write output: {}", e)))?;

    output.success(format!("Exported to: {}", output_path.display()));
    output.kv("Format", &format!("{:?}", format));

    if output.is_json() {
        let result = ExportResult {
            output_path: output_path.display().to_string(),
            format: format!("{:?}", format),
            symbols: codegraph.symbols.len(),
            edges: codegraph.call_graph.edges.len(),
        };
        output.json(&result)?;
    }

    Ok(())
}

/// Export call graph to CSV format
fn export_to_csv(codegraph: &CodeGraph) -> String {
    let mut csv = String::new();

    // CSV header
    csv.push_str("caller_id,caller_name,caller_kind,callee_id,callee_name,callee_kind,call_site,is_recursive,is_trait_call,is_generic_instantiation\n");

    // Export edges
    for edge in &codegraph.call_graph.edges {
        let caller = codegraph.symbols.get(&edge.caller);
        let callee = codegraph.symbols.get(&edge.callee);

        let caller_name = caller.map(|s| s.name.as_str()).unwrap_or("unknown");
        let caller_kind = caller
            .map(|s| s.kind.to_string())
            .unwrap_or("unknown".to_string());
        let callee_name = callee.map(|s| s.name.as_str()).unwrap_or("unknown");
        let callee_kind = callee
            .map(|s| s.kind.to_string())
            .unwrap_or("unknown".to_string());

        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            edge.caller.to_hex(),
            caller_name,
            caller_kind,
            edge.callee.to_hex(),
            callee_name,
            callee_kind,
            edge.call_site,
            edge.is_recursive,
            edge.is_trait_call,
            edge.is_generic_instantiation
        ));
    }

    csv
}

#[derive(Serialize)]
struct StatsResult {
    total_symbols: usize,
    total_edges: usize,
    recursive_edges: usize,
    trait_calls: usize,
    generic_instantiations: usize,
    max_callers: usize,
    max_callees: usize,
    unique_callers: usize,
    unique_callees: usize,
    content_hash: String,
}

/// Generate call graph statistics
pub async fn generate_stats(codegraph_path: &Path, output: &OutputWriter) -> Result<()> {
    output.section("CodeGraph Statistics");
    output.kv("Database", &codegraph_path.display().to_string());
    output.blank();

    // Load CodeGraph
    let codegraph = CodeGraph::load_from_db(codegraph_path).await?;

    // Get statistics
    let stats = codegraph.call_graph.statistics();

    output.section("Call Graph Statistics");
    output.kv("Total edges", &stats.total_edges.to_string());
    output.kv("Recursive edges", &stats.recursive_edges.to_string());
    output.kv("Trait calls", &stats.trait_calls.to_string());
    output.kv(
        "Generic instantiations",
        &stats.generic_instantiations.to_string(),
    );
    output.kv("Max callers", &stats.max_callers.to_string());
    output.kv("Max callees", &stats.max_callees.to_string());
    output.kv("Unique callers", &stats.unique_callers.to_string());
    output.kv("Unique callees", &stats.unique_callees.to_string());
    output.blank();

    output.section("Symbol Statistics");
    output.kv("Total symbols", &codegraph.symbols.len().to_string());

    // Count by kind
    let mut kind_counts = std::collections::BTreeMap::new();
    for symbol in codegraph.symbols.values() {
        *kind_counts.entry(symbol.kind.to_string()).or_insert(0) += 1;
    }

    for (kind, count) in kind_counts {
        output.kv(&kind, &count.to_string());
    }

    output.blank();
    output.kv("Content Hash", &codegraph.content_hash.to_hex());

    if output.is_json() {
        let result = StatsResult {
            total_symbols: codegraph.symbols.len(),
            total_edges: stats.total_edges,
            recursive_edges: stats.recursive_edges,
            trait_calls: stats.trait_calls,
            generic_instantiations: stats.generic_instantiations,
            max_callers: stats.max_callers,
            max_callees: stats.max_callees,
            unique_callers: stats.unique_callers,
            unique_callees: stats.unique_callees,
            content_hash: codegraph.content_hash.to_hex(),
        };
        output.json(&result)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_parsing() {
        assert_eq!(
            "dot".parse::<ExportFormat>().expect("dot should parse"),
            ExportFormat::Dot
        );
        assert_eq!(
            "json".parse::<ExportFormat>().expect("json should parse"),
            ExportFormat::Json
        );
        assert_eq!(
            "csv".parse::<ExportFormat>().expect("csv should parse"),
            ExportFormat::Csv
        );

        assert!("unknown".parse::<ExportFormat>().is_err());
    }

    #[test]
    fn test_csv_export() {
        let codegraph = CodeGraph::new();
        let csv = export_to_csv(&codegraph);

        assert!(csv.contains("caller_id,caller_name"));
        assert!(csv.contains("callee_id,callee_name"));
    }
}
