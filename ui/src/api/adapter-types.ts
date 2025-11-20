// Adapter-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-01-20†refactor†adapter_types】

export interface Adapter {
  id: string;
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  tier: number;
  languages_json?: string;
  framework?: string;

  // Semantic naming fields
  adapter_name?: string;           // Full semantic name: tenant/domain/purpose/r001
  tenant_namespace?: string;       // e.g., "shop-floor"
  domain?: string;                 // e.g., "hydraulics"
  purpose?: string;                // e.g., "troubleshooting"
  revision?: string;               // e.g., "r042"
  version?: string;                // e.g., "1.0.0"
  parent_id?: string;              // Parent adapter for lineage tracking
  fork_type?: 'independent' | 'extension';
  fork_reason?: string;

  // Code intelligence fields
  category: 'code' | 'framework' | 'codebase' | 'ephemeral';
  scope: 'global' | 'tenant' | 'repo' | 'commit';
  framework_id?: string;
  framework_version?: string;
  repo_id?: string;
  commit_sha?: string;
  intent?: string;

  // Lifecycle state management
  current_state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  lifecycle_state?: string;        // Alternative name for current_state
  pinned: boolean;
  memory_bytes: number;
  last_activated?: string;
  activation_count: number;

  created_at: string;
  updated_at: string;
  active: boolean;
}

export type AdapterCategory = 'code' | 'framework' | 'codebase' | 'ephemeral';
export type AdapterScope = 'global' | 'tenant' | 'repo' | 'commit';
export type AdapterState = 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
export type LifecycleState = 'draft' | 'active' | 'deprecated' | 'retired';
export type EvictionPriority = 'never' | 'low' | 'normal' | 'high' | 'critical';

export interface RegisterAdapterRequest {
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  tier: number;
  languages_json?: string;
  framework?: string;
  category: AdapterCategory;
  scope?: AdapterScope;
  expires_at?: string;
  metadata_json?: string;
}

export interface UpdateAdapterRequest {
  name?: string;
  tier?: number;
  expires_at?: string;
  metadata_json?: string;
}

export interface AdapterResponse {
  adapter: Adapter;
}

export interface ListAdaptersResponse {
  adapters: Adapter[];
  total: number;
  page: number;
  page_size: number;
}

export interface LoadAdapterRequest {
  adapter_id: string;
  priority?: EvictionPriority;
}

export interface UnloadAdapterRequest {
  adapter_id: string;
}

export interface AdapterLoadResponse {
  adapter_id: string;
  state: AdapterState;
  vram_mb?: number;
}

export interface AdapterFingerprintResponse {
  adapter_id: string;
  fingerprint: string;
  buffer_size: number;
  last_verified: string;
}

export interface ActiveAdapter {
  adapter_id: string;
  gate: number;  // Q15 quantized gate value
  priority?: EvictionPriority;
}

export interface AdapterStack {
  id: string;
  name: string;
  adapters: ActiveAdapter[];
  description?: string;
  created_at: string;
  updated_at: string;
}

export interface CreateAdapterStackRequest {
  name: string;
  adapters: ActiveAdapter[];
  description?: string;
}

export interface UpdateAdapterStackRequest {
  name?: string;
  adapters?: ActiveAdapter[];
  description?: string;
}

export interface AdapterStackResponse {
  stack: AdapterStack;
}

export interface ListAdapterStacksResponse {
  stacks: AdapterStack[];
  total: number;
}

// Re-export commonly used types for convenience
export type { Adapter as default };
