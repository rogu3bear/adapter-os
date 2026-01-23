//! Tauri commands for graph operations

use crate::types::*;
use adapteros_codegraph::{CodeGraph, CodeGraphDb, SymbolId};
use std::path::Path;
use tauri::command;

/// Load a CodeGraph from SQLite database
#[command]
pub async fn load_graph(db_path: String) -> Result<GraphData, String> {
    tracing::info!("Loading graph from: {}", db_path);

    let path = Path::new(&db_path);
    if !path.exists() {
        return Err(format!("Database file not found: {}", db_path));
    }

    let graph = CodeGraph::load_from_db(path)
        .await
        .map_err(|e| format!("Failed to load graph: {}", e))?;

    // Convert to frontend types
    let nodes: Vec<GraphNode> = graph
        .symbols
        .iter()
        .map(|(id, symbol)| GraphNode {
            id: id.to_hex(),
            name: symbol.name.clone(),
            kind: symbol.kind.to_string(),
            file_path: symbol.file_path.clone(),
            span: SpanData {
                start_line: symbol.span.start_line,
                start_column: symbol.span.start_column,
                end_line: symbol.span.end_line,
                end_column: symbol.span.end_column,
                byte_start: symbol.span.byte_start,
                byte_length: symbol.span.byte_length,
            },
            visibility: symbol.visibility.to_string(),
            has_type_annotation: symbol.type_annotation.is_some(),
            is_recursive: symbol.is_recursive,
            is_async: symbol.is_async,
            is_unsafe: symbol.is_unsafe,
            qualified_name: symbol.qualified_name(),
        })
        .collect();

    let edges: Vec<GraphEdge> = graph
        .call_graph
        .edges
        .iter()
        .map(|edge| GraphEdge {
            source: edge.caller.to_hex(),
            target: edge.callee.to_hex(),
            call_site: edge.call_site.clone(),
            is_recursive: edge.is_recursive,
            is_trait_call: edge.is_trait_call,
            is_generic_instantiation: edge.is_generic_instantiation,
        })
        .collect();

    let stats_data = graph.call_graph.statistics();
    let stats = GraphStats {
        node_count: graph.symbols.len(),
        edge_count: graph.call_graph.edges.len(),
        recursive_count: stats_data.recursive_edges,
        trait_call_count: stats_data.trait_calls,
        generic_instantiation_count: stats_data.generic_instantiations,
    };

    tracing::info!(
        "Loaded graph: {} nodes, {} edges",
        stats.node_count,
        stats.edge_count
    );

    Ok(GraphData {
        nodes,
        edges,
        stats,
    })
}

/// Search for symbols by name, qualified name, or file path
#[command]
pub async fn search_symbols(db_path: String, query: String) -> Result<Vec<SymbolMatch>, String> {
    tracing::debug!("Searching for: {}", query);

    let path = Path::new(&db_path);
    let graph = CodeGraph::load_from_db(path)
        .await
        .map_err(|e| format!("Failed to load graph: {}", e))?;

    let query_lower = query.to_lowercase();
    let matches: Vec<SymbolMatch> = graph
        .symbols
        .iter()
        .filter(|(_, symbol)| {
            symbol.name.to_lowercase().contains(&query_lower)
                || symbol
                    .qualified_name()
                    .to_lowercase()
                    .contains(&query_lower)
                || symbol.file_path.to_lowercase().contains(&query_lower)
        })
        .take(100) // Limit results
        .map(|(id, symbol)| SymbolMatch {
            id: id.to_hex(),
            name: symbol.name.clone(),
            kind: symbol.kind.to_string(),
            file_path: symbol.file_path.clone(),
            qualified_name: symbol.qualified_name(),
            span: SpanData {
                start_line: symbol.span.start_line,
                start_column: symbol.span.start_column,
                end_line: symbol.span.end_line,
                end_column: symbol.span.end_column,
                byte_start: symbol.span.byte_start,
                byte_length: symbol.span.byte_length,
            },
        })
        .collect();

    tracing::debug!("Found {} matches", matches.len());
    Ok(matches)
}

/// Get detailed information about a symbol
#[command]
pub async fn get_symbol_details(
    db_path: String,
    symbol_id: String,
) -> Result<SymbolDetails, String> {
    let path = Path::new(&db_path);
    let graph = CodeGraph::load_from_db(path)
        .await
        .map_err(|e| format!("Failed to load graph: {}", e))?;

    let id = SymbolId::from_hex(&symbol_id).map_err(|e| format!("Invalid symbol ID: {}", e))?;

    let symbol = graph
        .get_symbol(&id)
        .ok_or_else(|| format!("Symbol not found: {}", symbol_id))?;

    // Get callers and callees
    let callers: Vec<SymbolRef> = graph
        .get_callers(&id)
        .iter()
        .filter_map(|caller_id| {
            graph.get_symbol(caller_id).map(|s| SymbolRef {
                id: caller_id.to_hex(),
                name: s.name.clone(),
                kind: s.kind.to_string(),
            })
        })
        .collect();

    let callees: Vec<SymbolRef> = graph
        .get_callees(&id)
        .iter()
        .filter_map(|callee_id| {
            graph.get_symbol(callee_id).map(|s| SymbolRef {
                id: callee_id.to_hex(),
                name: s.name.clone(),
                kind: s.kind.to_string(),
            })
        })
        .collect();

    Ok(SymbolDetails {
        id: id.to_hex(),
        name: symbol.name.clone(),
        kind: symbol.kind.to_string(),
        qualified_name: symbol.qualified_name(),
        file_path: symbol.file_path.clone(),
        span: SpanData {
            start_line: symbol.span.start_line,
            start_column: symbol.span.start_column,
            end_line: symbol.span.end_line,
            end_column: symbol.span.end_column,
            byte_start: symbol.span.byte_start,
            byte_length: symbol.span.byte_length,
        },
        visibility: symbol.visibility.to_string(),
        type_annotation: symbol.type_annotation.as_ref().map(|ta| ta.to_string()),
        signature: symbol.signature.clone(),
        docstring: symbol.docstring.clone(),
        is_recursive: symbol.is_recursive,
        is_async: symbol.is_async,
        is_unsafe: symbol.is_unsafe,
        callers,
        callees,
    })
}

/// Get neighbors (callers and callees) of a symbol
#[command]
pub async fn get_neighbors(db_path: String, symbol_id: String) -> Result<Neighbors, String> {
    let path = Path::new(&db_path);
    let graph = CodeGraph::load_from_db(path)
        .await
        .map_err(|e| format!("Failed to load graph: {}", e))?;

    let id = SymbolId::from_hex(&symbol_id).map_err(|e| format!("Invalid symbol ID: {}", e))?;

    let callers: Vec<SymbolMatch> = graph
        .get_callers(&id)
        .iter()
        .filter_map(|caller_id| {
            graph.get_symbol(caller_id).map(|s| SymbolMatch {
                id: caller_id.to_hex(),
                name: s.name.clone(),
                kind: s.kind.to_string(),
                file_path: s.file_path.clone(),
                qualified_name: s.qualified_name(),
                span: SpanData {
                    start_line: s.span.start_line,
                    start_column: s.span.start_column,
                    end_line: s.span.end_line,
                    end_column: s.span.end_column,
                    byte_start: s.span.byte_start,
                    byte_length: s.span.byte_length,
                },
            })
        })
        .collect();

    let callees: Vec<SymbolMatch> = graph
        .get_callees(&id)
        .iter()
        .filter_map(|callee_id| {
            graph.get_symbol(callee_id).map(|s| SymbolMatch {
                id: callee_id.to_hex(),
                name: s.name.clone(),
                kind: s.kind.to_string(),
                file_path: s.file_path.clone(),
                qualified_name: s.qualified_name(),
                span: SpanData {
                    start_line: s.span.start_line,
                    start_column: s.span.start_column,
                    end_line: s.span.end_line,
                    end_column: s.span.end_column,
                    byte_start: s.span.byte_start,
                    byte_length: s.span.byte_length,
                },
            })
        })
        .collect();

    Ok(Neighbors { callers, callees })
}

/// Load diff between two graph databases
#[command]
pub async fn load_diff(db_path_a: String, db_path_b: String) -> Result<GraphDiffData, String> {
    tracing::info!("Computing diff between {} and {}", db_path_a, db_path_b);

    let path_a = Path::new(&db_path_a);
    let path_b = Path::new(&db_path_b);

    let graph_a = CodeGraph::load_from_db(path_a)
        .await
        .map_err(|e| format!("Failed to load graph A: {}", e))?;

    let graph_b = CodeGraph::load_from_db(path_b)
        .await
        .map_err(|e| format!("Failed to load graph B: {}", e))?;

    let diff = crate::diff::compute_diff(&graph_a, &graph_b);

    Ok(diff)
}

/// Open source file at specific line
#[command]
pub async fn open_source_file(file_path: String, line: u32) -> Result<(), String> {
    tracing::info!("Opening file: {} at line {}", file_path, line);

    // Verify file exists before attempting to open
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    // Use platform-specific approach to open files
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // On macOS, use 'open' to open in default editor
        // Note: 'open' doesn't support line numbers directly, but editors like
        // VS Code and Sublime Text can be configured to handle this
        Command::new("open")
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // On Windows, use 'start' command via cmd.exe to open in default application
        // The empty "" is for the window title parameter required by start
        Command::new("cmd")
            .args(["/C", "start", "", &file_path])
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        // On Linux, use 'xdg-open' which opens with the default application
        Command::new("xdg-open")
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        return Err("File opening not implemented for this platform".to_string());
    }

    // Note: The 'line' parameter is logged but not used directly in the open command
    // as most OS-level file open commands don't support jumping to a specific line.
    // IDE integrations (VS Code, etc.) would need custom URI schemes for line support.
    let _ = line; // Suppress unused warning

    Ok(())
}
