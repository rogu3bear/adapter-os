/**
 * Type guard utilities for runtime type checking.
 * Use these instead of `as any` casts with Array.includes().
 */

import type { AdapterCategory, AdapterState, AdapterHealthFlag } from '@/api/adapter-types';

// Adapter-related type guards
const ADAPTER_TABS = ['overview', 'activations', 'usage', 'lineage', 'manifest', 'register', 'policies'] as const;
export type AdaptersTab = typeof ADAPTER_TABS[number];

export function isAdaptersTab(value: string): value is AdaptersTab {
  return (ADAPTER_TABS as readonly string[]).includes(value);
}

const ADAPTER_CATEGORIES = ['code', 'framework', 'codebase', 'ephemeral'] as const;

export function isAdapterCategory(value: string | undefined | null): value is AdapterCategory {
  return value != null && (ADAPTER_CATEGORIES as readonly string[]).includes(value);
}

const ADAPTER_STATES = ['unloaded', 'loading', 'cold', 'warm', 'hot', 'resident', 'error'] as const;

export function isAdapterState(value: string | undefined | null): value is AdapterState {
  return value != null && (ADAPTER_STATES as readonly string[]).includes(value);
}

const ADAPTER_HEALTH_FLAGS = ['healthy', 'degraded', 'unsafe', 'corrupt', 'unknown'] as const;

export function isAdapterHealthFlag(value: string | undefined | null): value is AdapterHealthFlag | 'unknown' {
  return value != null && (ADAPTER_HEALTH_FLAGS as readonly string[]).includes(value);
}

// Filter mode for routes debug
const FILTER_MODES = ['all', 'issues', 'orphans', 'duplicates', 'hubs', 'deprecated', 'hidden'] as const;
export type FilterMode = typeof FILTER_MODES[number];

export function isFilterMode(value: string): value is FilterMode {
  return (FILTER_MODES as readonly string[]).includes(value);
}
