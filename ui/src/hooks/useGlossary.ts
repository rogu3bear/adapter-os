import { useMemo, useCallback } from 'react';
import {
  getGlossaryEntry,
  getBriefText,
  getByCategory,
  getRelatedTerms,
  searchGlossary,
  allEntries,
  type GlossaryEntry,
  type GlossaryCategory,
  type GlossarySearchResult,
} from '@/data/glossary';

export interface UseGlossaryReturn {
  // Single entry lookup
  getEntry: (id: string) => GlossaryEntry | undefined;

  // Brief text for tooltips
  getBrief: (id: string) => string | undefined;

  // Search
  search: (query: string, options?: { categories?: GlossaryCategory[]; limit?: number }) => GlossarySearchResult[];

  // Category browsing
  getByCategory: (category: GlossaryCategory) => GlossaryEntry[];

  // Relationships
  getRelated: (id: string) => GlossaryEntry[];

  // All entries for browsing
  allEntries: GlossaryEntry[];
}

/**
 * Hook for accessing AdapterOS glossary data.
 *
 * Provides a clean React API for:
 * - Looking up term definitions
 * - Getting brief text for tooltips
 * - Searching across terms
 * - Browsing by category
 * - Finding related terms
 *
 * @example
 * ```tsx
 * const { getEntry, getBrief, search } = useGlossary();
 *
 * // Get full entry
 * const entry = getEntry('lora');
 *
 * // Get brief text for tooltip
 * const brief = getBrief('aos');
 *
 * // Search
 * const results = search('adapter', { categories: ['Core'], limit: 5 });
 * ```
 */
export function useGlossary(): UseGlossaryReturn {
  // Memoize getEntry wrapper
  const getEntry = useCallback((id: string): GlossaryEntry | undefined => {
    return getGlossaryEntry(id);
  }, []);

  // Memoize getBrief wrapper
  const getBrief = useCallback((id: string): string | undefined => {
    return getBriefText(id);
  }, []);

  // Memoize search wrapper - transform matchType to matchedField for type compatibility
  const search = useCallback(
    (
      query: string,
      options?: { categories?: GlossaryCategory[]; limit?: number }
    ): GlossarySearchResult[] => {
      const results = searchGlossary(query, options);
      return results.map(r => ({
        entry: r.entry,
        score: r.score,
        matchedField: r.matchType as GlossarySearchResult['matchedField'],
      }));
    },
    []
  );

  // Memoize getByCategory wrapper
  const getByCategoryMemo = useCallback((category: GlossaryCategory): GlossaryEntry[] => {
    return getByCategory(category);
  }, []);

  // Memoize getRelated wrapper
  const getRelated = useCallback((id: string): GlossaryEntry[] => {
    return getRelatedTerms(id);
  }, []);

  // Memoize allEntries reference - cast to mutable for type compatibility
  const allEntriesMemo = useMemo(() => [...allEntries], []);

  return {
    getEntry,
    getBrief,
    search,
    getByCategory: getByCategoryMemo,
    getRelated,
    allEntries: allEntriesMemo,
  };
}
