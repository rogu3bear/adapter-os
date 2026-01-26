//! Graph diff computation

use crate::types::*;
use adapteros_retrieval::codegraph::CodeGraph;
use std::collections::HashSet;

/// Compute diff between two CodeGraphs
pub fn compute_diff(graph_a: &CodeGraph, graph_b: &CodeGraph) -> GraphDiffData {
    // Collect IDs for comparison
    let ids_a: HashSet<_> = graph_a.symbols.keys().collect();
    let ids_b: HashSet<_> = graph_b.symbols.keys().collect();

    // Nodes only in B = added
    let nodes_added: Vec<GraphNode> = ids_b
        .difference(&ids_a)
        .filter_map(|id| {
            graph_b.symbols.get(id).map(|symbol| GraphNode {
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
        })
        .collect();

    // Nodes only in A = removed
    let nodes_removed: Vec<GraphNode> = ids_a
        .difference(&ids_b)
        .filter_map(|id| {
            graph_a.symbols.get(id).map(|symbol| GraphNode {
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
        })
        .collect();

    // Nodes in both but potentially modified
    let mut nodes_modified = Vec::new();
    for id in ids_a.intersection(&ids_b) {
        if let (Some(symbol_a), Some(symbol_b)) =
            (graph_a.symbols.get(id), graph_b.symbols.get(id))
        {
            // Check if content changed (signature, type, etc.)
            let changed = symbol_a.signature != symbol_b.signature
                || symbol_a.type_annotation != symbol_b.type_annotation
                || symbol_a.visibility != symbol_b.visibility
                || symbol_a.is_async != symbol_b.is_async
                || symbol_a.is_unsafe != symbol_b.is_unsafe;

            if changed {
                let node_a = GraphNode {
                    id: id.to_hex(),
                    name: symbol_a.name.clone(),
                    kind: symbol_a.kind.to_string(),
                    file_path: symbol_a.file_path.clone(),
                    span: SpanData {
                        start_line: symbol_a.span.start_line,
                        start_column: symbol_a.span.start_column,
                        end_line: symbol_a.span.end_line,
                        end_column: symbol_a.span.end_column,
                        byte_start: symbol_a.span.byte_start,
                        byte_length: symbol_a.span.byte_length,
                    },
                    visibility: symbol_a.visibility.to_string(),
                    has_type_annotation: symbol_a.type_annotation.is_some(),
                    is_recursive: symbol_a.is_recursive,
                    is_async: symbol_a.is_async,
                    is_unsafe: symbol_a.is_unsafe,
                    qualified_name: symbol_a.qualified_name(),
                };

                let node_b = GraphNode {
                    id: id.to_hex(),
                    name: symbol_b.name.clone(),
                    kind: symbol_b.kind.to_string(),
                    file_path: symbol_b.file_path.clone(),
                    span: SpanData {
                        start_line: symbol_b.span.start_line,
                        start_column: symbol_b.span.start_column,
                        end_line: symbol_b.span.end_line,
                        end_column: symbol_b.span.end_column,
                        byte_start: symbol_b.span.byte_start,
                        byte_length: symbol_b.span.byte_length,
                    },
                    visibility: symbol_b.visibility.to_string(),
                    has_type_annotation: symbol_b.type_annotation.is_some(),
                    is_recursive: symbol_b.is_recursive,
                    is_async: symbol_b.is_async,
                    is_unsafe: symbol_b.is_unsafe,
                    qualified_name: symbol_b.qualified_name(),
                };

                nodes_modified.push((node_a, node_b));
            }
        }
    }

    // Compare edges
    let edges_a: HashSet<_> = graph_a
        .call_graph
        .edges
        .iter()
        .map(|e| (e.caller.to_hex(), e.callee.to_hex()))
        .collect();

    let edges_b: HashSet<_> = graph_b
        .call_graph
        .edges
        .iter()
        .map(|e| (e.caller.to_hex(), e.callee.to_hex()))
        .collect();

    let edges_added: Vec<GraphEdge> = graph_b
        .call_graph
        .edges
        .iter()
        .filter(|e| !edges_a.contains(&(e.caller.to_hex(), e.callee.to_hex())))
        .map(|edge| GraphEdge {
            source: edge.caller.to_hex(),
            target: edge.callee.to_hex(),
            call_site: edge.call_site.clone(),
            is_recursive: edge.is_recursive,
            is_trait_call: edge.is_trait_call,
            is_generic_instantiation: edge.is_generic_instantiation,
        })
        .collect();

    let edges_removed: Vec<GraphEdge> = graph_a
        .call_graph
        .edges
        .iter()
        .filter(|e| !edges_b.contains(&(e.caller.to_hex(), e.callee.to_hex())))
        .map(|edge| GraphEdge {
            source: edge.caller.to_hex(),
            target: edge.callee.to_hex(),
            call_site: edge.call_site.clone(),
            is_recursive: edge.is_recursive,
            is_trait_call: edge.is_trait_call,
            is_generic_instantiation: edge.is_generic_instantiation,
        })
        .collect();

    let stats = DiffStats {
        nodes_added: nodes_added.len(),
        nodes_removed: nodes_removed.len(),
        nodes_modified: nodes_modified.len(),
        edges_added: edges_added.len(),
        edges_removed: edges_removed.len(),
    };

    GraphDiffData {
        nodes_added,
        nodes_removed,
        nodes_modified,
        edges_added,
        edges_removed,
        stats,
    }
}

