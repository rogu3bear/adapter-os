/**
 * Unified Glossary Types
 *
 * Single source of truth for all help text, tooltips, and concept definitions.
 * Consolidates concept-tooltips.ts, help-text.ts, and help-tooltip.tsx fallbacks.
 */

/**
 * Categories for organizing glossary terms
 */
export type GlossaryCategory =
  | 'core-concepts'  // tenant, adapter, stack, router, kernel, base-model
  | 'lifecycle'      // tiers, eviction, pinning, states
  | 'routing'        // k-sparse, gates, entropy, overhead
  | 'training'       // rank, alpha, learning-rate, epochs, datasets
  | 'inference'      // temperature, top-k, max-tokens, streaming
  | 'security'       // policies, isolation, compliance, audit
  | 'system'         // nodes, workers, memory, metrics
  | 'ui-fields';     // form-specific help text

/**
 * Tiered content structure for progressive disclosure
 */
export interface GlossaryContent {
  /** Brief tooltip text (1-2 sentences, ~150 chars max) */
  brief: string;

  /** Detailed explanation shown in GlossarySheet (markdown supported) */
  detailed?: string;
}

/**
 * Main glossary entry interface
 */
export interface GlossaryEntry {
  /** Unique identifier in kebab-case (e.g., "lora-rank", "k-sparse-routing") */
  id: string;

  /** Display term (e.g., "LoRA Rank", "K-Sparse Routing") */
  term: string;

  /** Primary category for organization */
  category: GlossaryCategory;

  /** Tiered content */
  content: GlossaryContent;

  /** Related term IDs for navigation */
  relatedTerms?: string[];

  /** Alternative names/synonyms for search (e.g., ["rank", "adapter rank"]) */
  aliases?: string[];
}

/**
 * Category metadata for UI display
 */
export interface CategoryMeta {
  id: GlossaryCategory;
  label: string;
  description: string;
  icon: string; // Lucide icon name
}

/**
 * Search result with relevance scoring
 */
export interface GlossarySearchResult {
  entry: GlossaryEntry;
  score: number;
  matchedField: 'term' | 'alias' | 'brief' | 'detailed';
}

/**
 * Category metadata definitions
 */
export const categoryMeta: Record<GlossaryCategory, CategoryMeta> = {
  'core-concepts': {
    id: 'core-concepts',
    label: 'Core Concepts',
    description: 'Fundamental entities in AdapterOS',
    icon: 'Box',
  },
  lifecycle: {
    id: 'lifecycle',
    label: 'Lifecycle',
    description: 'Adapter state management and memory tiers',
    icon: 'RefreshCw',
  },
  routing: {
    id: 'routing',
    label: 'Routing',
    description: 'K-sparse routing and adapter selection',
    icon: 'Route',
  },
  training: {
    id: 'training',
    label: 'Training',
    description: 'LoRA training parameters and datasets',
    icon: 'Zap',
  },
  inference: {
    id: 'inference',
    label: 'Inference',
    description: 'Model inference parameters and streaming',
    icon: 'Play',
  },
  security: {
    id: 'security',
    label: 'Security',
    description: 'Policies, isolation, and compliance',
    icon: 'Shield',
  },
  system: {
    id: 'system',
    label: 'System',
    description: 'Nodes, workers, memory, and metrics',
    icon: 'Server',
  },
  'ui-fields': {
    id: 'ui-fields',
    label: 'UI Fields',
    description: 'Form fields and UI-specific help',
    icon: 'FormInput',
  },
};
