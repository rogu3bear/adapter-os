import type { GraphData, SymbolDetails } from '../types/graph';

export const graphFixture: GraphData = {
  nodes: [
    {
      id: 'node-1',
      name: 'parse_request',
      kind: 'Function',
      file_path: 'src/api/handler.rs',
      span: {
        start_line: 10,
        start_column: 1,
        end_line: 42,
        end_column: 1,
        byte_start: 0,
        byte_length: 128,
      },
      visibility: 'pub',
      has_type_annotation: true,
      is_recursive: false,
      is_async: true,
      is_unsafe: false,
      qualified_name: 'adapteros::api::parse_request',
    },
    {
      id: 'node-2',
      name: 'validate_payload',
      kind: 'Function',
      file_path: 'src/api/validate.rs',
      span: {
        start_line: 5,
        start_column: 1,
        end_line: 20,
        end_column: 1,
        byte_start: 0,
        byte_length: 64,
      },
      visibility: 'pub',
      has_type_annotation: true,
      is_recursive: false,
      is_async: false,
      is_unsafe: false,
      qualified_name: 'adapteros::api::validate_payload',
    },
  ],
  edges: [
    {
      source: 'node-1',
      target: 'node-2',
      call_site: 'src/api/handler.rs:18',
      is_recursive: false,
      is_trait_call: false,
      is_generic_instantiation: false,
    },
  ],
  stats: {
    node_count: 2,
    edge_count: 1,
    recursive_count: 0,
    trait_call_count: 0,
    generic_instantiation_count: 0,
  },
};

export const detailsFixture: SymbolDetails = {
  id: 'node-1',
  name: 'parse_request',
  kind: 'Function',
  qualified_name: 'adapteros::api::parse_request',
  file_path: 'src/api/handler.rs',
  span: {
    start_line: 10,
    start_column: 1,
    end_line: 42,
    end_column: 1,
    byte_start: 0,
    byte_length: 128,
  },
  visibility: 'pub',
  type_annotation: 'fn(Request) -> Response',
  signature: 'pub async fn parse_request(req: Request) -> Response',
  docstring: 'Parses and validates API requests.',
  is_recursive: false,
  is_async: true,
  is_unsafe: false,
  callers: [
    {
      id: 'node-2',
      name: 'validate_payload',
      kind: 'Function',
    },
  ],
  callees: [
    {
      id: 'node-2',
      name: 'validate_payload',
      kind: 'Function',
    },
  ],
};
