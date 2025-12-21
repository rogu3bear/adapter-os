/**
 * Adapter State Helpers
 *
 * Consolidated helper functions for adapter lifecycle states.
 * Replaces duplicated implementations in:
 * - components/Adapters.tsx
 * - components/AdapterLifecycleManager.tsx
 * - components/AdapterStateVisualization.tsx
 * - components/AdapterMemoryMonitor.tsx
 */

import type { LucideIcon } from 'lucide-react';
import {
  Square,
  Snowflake,
  Thermometer,
  Flame,
  Anchor,
  Activity,
  Minus,
} from 'lucide-react';

/**
 * Adapter lifecycle states
 */
export type AdapterState = 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';

/**
 * State configuration with icon, color, and label
 */
interface StateConfig {
  icon: LucideIcon;
  /** Badge/background color class */
  colorClass: string;
  /** Icon color class */
  iconColorClass: string;
  /** Human-readable label */
  label: string;
  /** Description for tooltips */
  description: string;
}

/**
 * State configurations - single source of truth for all state styling
 */
const STATE_CONFIGS: Record<AdapterState, StateConfig> = {
  unloaded: {
    icon: Square,
    colorClass: 'bg-gray-100 text-gray-800',
    iconColorClass: 'text-gray-500',
    label: 'Not Loaded',
    description: 'Adapter is not loaded into memory',
  },
  cold: {
    icon: Snowflake,
    colorClass: 'bg-blue-100 text-blue-800',
    iconColorClass: 'text-blue-500',
    label: 'Ready',
    description: 'Adapter weights on disk, ready to load',
  },
  warm: {
    icon: Thermometer,
    colorClass: 'bg-orange-100 text-orange-800',
    iconColorClass: 'text-orange-500',
    label: 'Standby',
    description: 'Adapter loaded, ready to activate',
  },
  hot: {
    icon: Flame,
    colorClass: 'bg-red-100 text-red-800',
    iconColorClass: 'text-red-500',
    label: 'Loaded',
    description: 'Adapter active and serving requests',
  },
  resident: {
    icon: Anchor,
    colorClass: 'bg-purple-100 text-purple-800',
    iconColorClass: 'text-purple-500',
    label: 'Pinned',
    description: 'Adapter pinned in memory, never evicted',
  },
};

/**
 * Get the Lucide icon component for an adapter state
 */
export function getAdapterStateIcon(state: AdapterState | string): LucideIcon {
  const config = STATE_CONFIGS[state as AdapterState];
  return config?.icon ?? Activity;
}

/**
 * Get the color class for an adapter state badge
 */
export function getAdapterStateColor(state: AdapterState | string): string {
  const config = STATE_CONFIGS[state as AdapterState];
  return config?.colorClass ?? 'bg-gray-100 text-gray-800';
}

/**
 * Get the icon color class for an adapter state
 */
export function getAdapterStateIconColor(state: AdapterState | string): string {
  const config = STATE_CONFIGS[state as AdapterState];
  return config?.iconColorClass ?? 'text-gray-500';
}

/**
 * Get the human-readable label for an adapter state
 */
export function getAdapterStateLabel(state: AdapterState | string): string {
  const config = STATE_CONFIGS[state as AdapterState];
  return config?.label ?? state;
}

/**
 * Get the description for an adapter state (for tooltips)
 */
export function getAdapterStateDescription(state: AdapterState | string): string {
  const config = STATE_CONFIGS[state as AdapterState];
  return config?.description ?? 'Unknown state';
}

/**
 * Get all state configuration for a given state
 */
export function getAdapterStateConfig(state: AdapterState | string): StateConfig | null {
  return STATE_CONFIGS[state as AdapterState] ?? null;
}

/**
 * Check if a state is "active" (warm, hot, or resident)
 */
export function isAdapterStateActive(state: AdapterState | string): boolean {
  return ['warm', 'hot', 'resident'].includes(state);
}

/**
 * Check if a state is "loaded" (not unloaded)
 */
export function isAdapterStateLoaded(state: AdapterState | string): boolean {
  return state !== 'unloaded';
}

/**
 * Get ordered list of all states (for display purposes)
 */
export function getAllAdapterStates(): AdapterState[] {
  return ['unloaded', 'cold', 'warm', 'hot', 'resident'];
}
