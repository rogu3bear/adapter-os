/**
 * Adapter Category Helpers
 *
 * Consolidated helper functions for adapter categories.
 * Replaces duplicated implementations across multiple components.
 */

import type { LucideIcon } from 'lucide-react';
import {
  Code,
  Layers,
  GitBranch,
  Clock,
  Target,
  Zap,
  Activity,
} from 'lucide-react';

/**
 * Adapter categories
 */
export type AdapterCategory = 'code' | 'framework' | 'codebase' | 'ephemeral';

/**
 * Category configuration with icon, color, and label
 */
interface CategoryConfig {
  icon: LucideIcon;
  /** Badge/background color class */
  colorClass: string;
  /** Human-readable label */
  label: string;
  /** Description for tooltips */
  description: string;
}

/**
 * Category configurations - single source of truth for all category styling
 */
const CATEGORY_CONFIGS: Record<AdapterCategory, CategoryConfig> = {
  code: {
    icon: Code,
    colorClass: 'bg-green-100 text-green-800',
    label: 'Code',
    description: 'General code assistance adapter',
  },
  framework: {
    icon: Layers,
    colorClass: 'bg-blue-100 text-blue-800',
    label: 'Framework',
    description: 'Framework-specific adapter (React, Vue, etc.)',
  },
  codebase: {
    icon: GitBranch,
    colorClass: 'bg-purple-100 text-purple-800',
    label: 'Codebase',
    description: 'Trained on a specific codebase',
  },
  ephemeral: {
    icon: Clock,
    colorClass: 'bg-yellow-100 text-yellow-800',
    label: 'Ephemeral',
    description: 'Temporary adapter, auto-expires',
  },
};

/**
 * Get the Lucide icon component for an adapter category
 */
export function getCategoryIcon(category: AdapterCategory | string): LucideIcon {
  const config = CATEGORY_CONFIGS[category as AdapterCategory];
  return config?.icon ?? Code;
}

/**
 * Get the color class for an adapter category badge
 */
export function getCategoryColor(category: AdapterCategory | string): string {
  const config = CATEGORY_CONFIGS[category as AdapterCategory];
  return config?.colorClass ?? 'bg-gray-100 text-gray-800';
}

/**
 * Get the human-readable label for an adapter category
 */
export function getCategoryLabel(category: AdapterCategory | string): string {
  const config = CATEGORY_CONFIGS[category as AdapterCategory];
  return config?.label ?? category;
}

/**
 * Get the description for an adapter category (for tooltips)
 */
export function getCategoryDescription(category: AdapterCategory | string): string {
  const config = CATEGORY_CONFIGS[category as AdapterCategory];
  return config?.description ?? 'Unknown category';
}

/**
 * Get all category configuration for a given category
 */
export function getCategoryConfig(category: AdapterCategory | string): CategoryConfig | null {
  return CATEGORY_CONFIGS[category as AdapterCategory] ?? null;
}

/**
 * Check if a category is ephemeral (auto-expires)
 */
export function isCategoryEphemeral(category: AdapterCategory | string): boolean {
  return category === 'ephemeral';
}

/**
 * Get ordered list of all categories (for display purposes)
 */
export function getAllCategories(): AdapterCategory[] {
  return ['code', 'framework', 'codebase', 'ephemeral'];
}
