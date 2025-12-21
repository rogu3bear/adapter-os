/**
 * Strict type definitions for Adapter API responses.
 * Separates required fields (must always exist) from optional fields.
 */

// Required fields - API must always return these
export interface AdapterRequired {
  adapter_id: string;
  name: string;
}

// Optional fields - may be missing in some contexts
export interface AdapterOptional {
  description?: string;
  category?: 'code' | 'framework' | 'codebase' | 'ephemeral';
  current_state?: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  lifecycle_state?: string;
  runtime_state?: string;
  memory_bytes?: number;
  activation_count?: number;
  last_activated?: string;
  created_at?: string;
  updated_at?: string;
  pinned?: boolean;
  tenant_id?: string;
  repo_id?: string;
  version?: string;
  base_model_id?: string;
}

// Full adapter type
export type AdapterStrict = AdapterRequired & AdapterOptional;

// Type guard for required fields
export function hasRequiredAdapterFields(obj: unknown): obj is AdapterRequired {
  return (
    typeof obj === 'object' &&
    obj !== null &&
    'adapter_id' in obj &&
    typeof (obj as Record<string, unknown>).adapter_id === 'string' &&
    'name' in obj &&
    typeof (obj as Record<string, unknown>).name === 'string'
  );
}

// Assertion function for API boundaries
export function assertAdapter(obj: unknown, context?: string): asserts obj is AdapterStrict {
  if (!hasRequiredAdapterFields(obj)) {
    const ctx = context ? ` (${context})` : '';
    throw new Error(`Invalid adapter response${ctx}: missing required fields`);
  }
}
