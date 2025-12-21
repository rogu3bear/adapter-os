// TypeScript types matching Rust backend types

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
  stats: GraphStats;
}

export interface GraphNode {
  id: string;
  name: string;
  kind: string;
  file_path: string;
  span: SpanData;
  visibility: string;
  has_type_annotation: boolean;
  is_recursive: boolean;
  is_async: boolean;
  is_unsafe: boolean;
  qualified_name: string;
}

export interface GraphEdge {
  source: string;
  target: string;
  call_site: string;
  is_recursive: boolean;
  is_trait_call: boolean;
  is_generic_instantiation: boolean;
}

export interface SpanData {
  start_line: number;
  start_column: number;
  end_line: number;
  end_column: number;
  byte_start: number;
  byte_length: number;
}

export interface GraphStats {
  node_count: number;
  edge_count: number;
  recursive_count: number;
  trait_call_count: number;
  generic_instantiation_count: number;
}

export interface SymbolMatch {
  id: string;
  name: string;
  kind: string;
  file_path: string;
  qualified_name: string;
  span: SpanData;
}

export interface SymbolDetails {
  id: string;
  name: string;
  kind: string;
  qualified_name: string;
  file_path: string;
  span: SpanData;
  visibility: string;
  type_annotation: string | null;
  signature: string | null;
  docstring: string | null;
  is_recursive: boolean;
  is_async: boolean;
  is_unsafe: boolean;
  callers: SymbolRef[];
  callees: SymbolRef[];
}

export interface SymbolRef {
  id: string;
  name: string;
  kind: string;
}

export interface Neighbors {
  callers: SymbolMatch[];
  callees: SymbolMatch[];
}

export interface GraphDiffData {
  nodes_added: GraphNode[];
  nodes_removed: GraphNode[];
  nodes_modified: [GraphNode, GraphNode][];
  edges_added: GraphEdge[];
  edges_removed: GraphEdge[];
  stats: DiffStats;
}

export interface DiffStats {
  nodes_added: number;
  nodes_removed: number;
  nodes_modified: number;
  edges_added: number;
  edges_removed: number;
}

export type LayoutType = 'cola' | 'dagre' | 'circle' | 'grid';
export type DiffMode = 'none' | 'added' | 'removed' | 'modified' | 'all';

