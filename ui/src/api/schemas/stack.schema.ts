/**
 * Strict type definitions for AdapterStack API responses.
 */

export interface StackRequired {
  stack_id: string;
  name: string;
}

export interface StackOptional {
  description?: string;
  tenant_id?: string;
  adapter_ids?: string[];
  adapters?: Array<{ adapter_id: string; weight?: number }>;
  created_at?: string;
  updated_at?: string;
  is_active?: boolean;
}

export type AdapterStackStrict = StackRequired & StackOptional;

export function hasRequiredStackFields(obj: unknown): obj is StackRequired {
  return (
    typeof obj === 'object' &&
    obj !== null &&
    'stack_id' in obj &&
    typeof (obj as Record<string, unknown>).stack_id === 'string' &&
    'name' in obj &&
    typeof (obj as Record<string, unknown>).name === 'string'
  );
}

export function assertStack(obj: unknown, context?: string): asserts obj is AdapterStackStrict {
  if (!hasRequiredStackFields(obj)) {
    const ctx = context ? ` (${context})` : '';
    throw new Error(`Invalid stack response${ctx}: missing required fields`);
  }
}
