/**
 * Glossary Index
 *
 * Central export point for the unified glossary system.
 * Provides search, lookup, and backward compatibility with legacy tooltip systems.
 */

import { coreConceptsEntries } from './entries/core-concepts';
import { lifecycleEntries } from './entries/lifecycle';
import { routingEntries } from './entries/routing';
import { trainingEntries } from './entries/training';
import { inferenceEntries } from './entries/inference';
import { securityEntries } from './entries/security';
import { systemEntries } from './entries/system';
import { uiFieldsEntries } from './entries/ui-fields';

// Re-export types
export type {
  GlossaryEntry,
  GlossaryCategory,
  GlossaryContent,
  GlossarySearchResult,
} from './types';

// Re-export categoryMeta for use in components
export { categoryMeta } from './types';

// ============================================================================
// Combined Glossary
// ============================================================================

// Combine all entries (may contain duplicates from different category files)
const rawEntries = [
  ...coreConceptsEntries,
  ...lifecycleEntries,
  ...routingEntries,
  ...trainingEntries,
  ...inferenceEntries,
  ...securityEntries,
  ...systemEntries,
  ...uiFieldsEntries,
];

// Deduplicate by ID (first occurrence wins, warn in development)
const seenIds = new Set<string>();
const deduplicatedEntries: typeof rawEntries = [];

for (const entry of rawEntries) {
  if (seenIds.has(entry.id)) {
    if (process.env.NODE_ENV === 'development') {
      // eslint-disable-next-line no-console -- dev-mode duplicate detection warning
      console.warn(`[Glossary] Duplicate entry ID: "${entry.id}" - using first occurrence`);
    }
    continue;
  }
  seenIds.add(entry.id);
  deduplicatedEntries.push(entry);
}

export const allEntries = deduplicatedEntries as unknown as typeof rawEntries;

// Fast lookup by ID
export const glossaryById = new Map(
  allEntries.map(entry => [entry.id, entry])
);

// ============================================================================
// Lookup Functions
// ============================================================================

/**
 * Get a glossary entry by ID
 */
export function getGlossaryEntry(id: string) {
  return glossaryById.get(id);
}

/**
 * Get just the brief text for a term (useful for tooltips)
 */
export function getBriefText(id: string): string | undefined {
  return glossaryById.get(id)?.content.brief;
}

/**
 * Get all entries in a category
 */
export function getByCategory(category: string) {
  return allEntries.filter(entry => entry.category === category);
}

/**
 * Get related terms for an entry
 */
export function getRelatedTerms(id: string) {
  const entry = glossaryById.get(id);
  if (!entry?.relatedTerms) return [];

  return entry.relatedTerms
    .map(rt => glossaryById.get(rt))
    .filter((e): e is NonNullable<typeof e> => e !== undefined);
}

/**
 * Get all entries that reference a given term
 */
export function getBacklinks(id: string) {
  return allEntries.filter(entry =>
    entry.relatedTerms?.some(rt => rt === id)
  );
}

// ============================================================================
// Search
// ============================================================================

export interface SearchOptions {
  categories?: string[];
  limit?: number;
  includeAliases?: boolean;
}

interface ScoredEntry {
  entry: typeof allEntries[number];
  score: number;
  matchType: 'exact' | 'starts-with' | 'alias' | 'contains' | 'brief' | 'detailed';
}

/**
 * Search the glossary with relevance scoring
 *
 * Scoring:
 * - Exact term match: 100
 * - Term starts with query: 80
 * - Alias match: 70
 * - Term contains query: 60
 * - Brief content match: 40
 * - Detailed content match: 20
 */
export function searchGlossary(
  query: string,
  options: SearchOptions = {}
): Array<{
  entry: typeof allEntries[number];
  score: number;
  matchType: string;
}> {
  const {
    categories,
    limit = 50,
    includeAliases = true,
  } = options;

  if (!query.trim()) return [];

  const normalizedQuery = query.toLowerCase().trim();
  const scoredEntries: ScoredEntry[] = [];

  for (const entry of allEntries) {
    // Filter by category if specified
    if (categories && !categories.includes(entry.category)) {
      continue;
    }

    const normalizedTerm = entry.term.toLowerCase();
    let score = 0;
    let matchType: ScoredEntry['matchType'] = 'detailed';

    // Exact term match
    if (normalizedTerm === normalizedQuery) {
      score = 100;
      matchType = 'exact';
    }
    // Term starts with query
    else if (normalizedTerm.startsWith(normalizedQuery)) {
      score = 80;
      matchType = 'starts-with';
    }
    // Alias match
    else if (includeAliases && entry.aliases) {
      const aliasMatch = entry.aliases.some(
        alias => alias.toLowerCase() === normalizedQuery
      );
      if (aliasMatch) {
        score = 70;
        matchType = 'alias';
      }
    }
    // Term contains query
    if (score === 0 && normalizedTerm.includes(normalizedQuery)) {
      score = 60;
      matchType = 'contains';
    }

    // Brief content match
    if (score === 0) {
      const normalizedBrief = entry.content.brief.toLowerCase();
      if (normalizedBrief.includes(normalizedQuery)) {
        score = 40;
        matchType = 'brief';
      }
    }

    // Detailed content match
    if (score === 0 && entry.content.detailed) {
      const normalizedDetailed = entry.content.detailed.toLowerCase();
      if (normalizedDetailed.includes(normalizedQuery)) {
        score = 20;
        matchType = 'detailed';
      }
    }

    if (score > 0) {
      scoredEntries.push({ entry, score, matchType });
    }
  }

  // Sort by score descending, then alphabetically
  scoredEntries.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return a.entry.term.localeCompare(b.entry.term);
  });

  return scoredEntries.slice(0, limit);
}

/**
 * Get suggestions for autocomplete (optimized for prefix matching)
 */
export function getSuggestions(prefix: string, limit = 10) {
  if (!prefix.trim()) return [];

  const normalizedPrefix = prefix.toLowerCase().trim();
  const suggestions: Array<{ term: string; id: string; category: string }> = [];

  for (const entry of allEntries) {
    if (entry.term.toLowerCase().startsWith(normalizedPrefix)) {
      suggestions.push({
        term: entry.term,
        id: entry.id,
        category: entry.category,
      });
    }

    if (suggestions.length >= limit) break;
  }

  return suggestions;
}

// ============================================================================
// Legacy Compatibility Mappings
// ============================================================================

/**
 * Map legacy camelCase IDs from concept-tooltips.ts to new kebab-case IDs
 */
const legacyIdMappings: Record<string, string> = {
  // Core concepts
  'goldenRun': 'golden-run',
  'kSparse': 'k-sparse-routing',
  'deterministicExecution': 'deterministic-execution',
  'workflowType': 'workflow-type',
  'activationPercent': 'activation-percent',
  'merkleChain': 'merkle-chain',
  'baseModel': 'base-model',
  'aosSandbox': 'aos-sandbox',
  'tenantIsolation': 'tenant-isolation',

  // Lifecycle states
  'unloadedState': 'unloaded',
  'coldState': 'cold',
  'warmState': 'warm',
  'hotState': 'hot',
  'residentState': 'resident',
  'errorState': 'error',

  // Routing
  'expertCount': 'expert-count',
  'topK': 'top-k',
  'gateVector': 'gate-vector',
  'routingLatency': 'routing-latency',

  // Training
  'rankValue': 'lora-rank',
  'alphaValue': 'lora-alpha',
  'trainingSteps': 'training-steps',
  'learningRate': 'learning-rate',
  'batchSize': 'batch-size',
  'validationSplit': 'validation-split',
  'baseModelRef': 'base-model',
  'configHash': 'config-hash',
  'datasetQuality': 'dataset-quality',

  // Inference
  'inferenceLatency': 'inference-latency',
  'tokensPerSecond': 'tokens-per-second',
  'maxTokens': 'max-tokens',
  'temperature': 'temperature',
  'topP': 'top-p',

  // Security
  'policyPack': 'policy-pack',
  'auditLog': 'audit-log',
  'cryptoAudit': 'crypto-audit',
  'evidenceChain': 'evidence-chain',
  'signatureVerification': 'signature-verification',

  // System
  'memoryPressure': 'memory-pressure',
  'headroomPercent': 'headroom-percent',
  'evictionPolicy': 'eviction-policy',
  'workerCount': 'worker-count',
  'queueDepth': 'queue-depth',

  // UI Fields (for HelpTooltip)
  'adapter-name': 'adapter-name',
  'adapter-domain': 'adapter-domain',
  'adapter-purpose': 'adapter-purpose',
  'adapter-revision': 'adapter-revision',
  'training-template': 'training-template',
  'dataset-path': 'dataset-path',
  'output-path': 'output-path',
  'stack-name': 'stack-name',
  'tenant-id': 'tenant-id',
};

/**
 * Resolve a potentially legacy ID to current ID
 */
function resolveLegacyId(id: string): string {
  return legacyIdMappings[id] || id;
}

// ============================================================================
// Backward Compatibility Wrappers
// ============================================================================

/**
 * Get tooltip data in the legacy ConceptTooltip format
 * Used for gradual migration from concept-tooltips.ts
 */
export function getConceptTooltip(key: string) {
  const resolvedId = resolveLegacyId(key);
  const entry = getGlossaryEntry(resolvedId);

  if (!entry) return undefined;

  return {
    term: entry.term,
    definition: entry.content.brief,
    learnMoreUrl: undefined, // Sheet-only now, no external URLs
  };
}

/**
 * Get help text in the legacy HelpTooltip format
 * Used for gradual migration from form field tooltips
 */
export function getHelpTooltipText(id: string): string | undefined {
  const resolvedId = resolveLegacyId(id);
  return getBriefText(resolvedId);
}

/**
 * Check if a concept exists (for conditional rendering)
 */
export function hasGlossaryEntry(id: string): boolean {
  const resolvedId = resolveLegacyId(id);
  return glossaryById.has(resolvedId);
}

/**
 * Get all available categories
 */
export function getAllCategories(): string[] {
  const categories = new Set(allEntries.map(e => e.category));
  return Array.from(categories).sort();
}

/**
 * Get category display name
 */
export function getCategoryDisplayName(category: string): string {
  const displayNames: Record<string, string> = {
    'core-concepts': 'Core Concepts',
    'lifecycle': 'Lifecycle',
    'routing': 'Routing',
    'training': 'Training',
    'inference': 'Inference',
    'security': 'Security & Policy',
    'system': 'System & Performance',
    'ui-fields': 'UI Fields',
  };
  return displayNames[category] || category;
}

/**
 * Validate glossary integrity (for development/testing)
 */
export function validateGlossary(): {
  valid: boolean;
  errors: string[];
  warnings: string[];
} {
  const errors: string[] = [];
  const warnings: string[] = [];
  const seenIds = new Set<string>();

  for (const entry of allEntries) {
    // Check for duplicate IDs
    if (seenIds.has(entry.id)) {
      errors.push(`Duplicate ID: ${entry.id}`);
    }
    seenIds.add(entry.id);

    // Check related terms exist
    if (entry.relatedTerms) {
      for (const rt of entry.relatedTerms) {
        if (!glossaryById.has(rt)) {
          errors.push(
            `Entry "${entry.id}" references non-existent term: ${rt}`
          );
        }
      }
    }

    // Warn if no brief content
    if (!entry.content.brief) {
      warnings.push(`Entry "${entry.id}" has no brief content`);
    }

    // Warn if term is very long (might not fit in UI)
    if (entry.term.length > 50) {
      warnings.push(`Entry "${entry.id}" has very long term (${entry.term.length} chars)`);
    }
  }

  return {
    valid: errors.length === 0,
    errors,
    warnings,
  };
}

// ============================================================================
// Development Exports
// ============================================================================

// Export individual entry arrays for direct access if needed
export {
  coreConceptsEntries,
  lifecycleEntries,
  routingEntries,
  trainingEntries,
  inferenceEntries,
  securityEntries,
  systemEntries,
  uiFieldsEntries,
};
