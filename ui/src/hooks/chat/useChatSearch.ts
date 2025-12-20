// Chat Search Hook with FTS (Full-Text Search) support
// Uses TanStack Query for efficient search with debouncing
// Based on patterns from useTraining.ts
//
// Usage:
// ```tsx
// const { results, isSearching, error } = useChatSearch(
//   searchQuery,
//   { scope: 'all', limit: 20 }
// );
// ```

import { useQuery, UseQueryOptions } from '@tanstack/react-query';
import { useMemo } from 'react';
import { apiClient } from '@/api/services';
import type { ChatSearchResult, SearchSessionsQuery } from '@/api/chat-types';
import { useDebounce } from '@/hooks/ui/useDebouncedValue';
import { logger } from '@/utils/logger';

const QUERY_KEYS = {
  search: (query: string, options?: Omit<SearchSessionsQuery, 'q'>) =>
    ['chat', 'search', query, options] as const,
};

export interface UseChatSearchOptions extends Omit<SearchSessionsQuery, 'q'> {
  /**
   * Debounce delay in milliseconds
   * @default 300
   */
  debounceDelay?: number;
  /**
   * Minimum query length to trigger search
   * @default 2
   */
  minLength?: number;
  /**
   * Maximum query length to prevent URL length issues
   * @default 500
   */
  maxLength?: number;
  /**
   * Enable the query (overrides minLength check)
   * @default undefined
   */
  enabled?: boolean;
}

export interface UseChatSearchReturn {
  /**
   * Search results from the API
   */
  results: ChatSearchResult[];
  /**
   * Whether the search is currently loading
   */
  isSearching: boolean;
  /**
   * Whether a debounce is pending (query changed but not yet executed)
   */
  isPending: boolean;
  /**
   * Error if the search failed
   */
  error: Error | null;
  /**
   * Whether the search query is valid (meets minLength requirement)
   */
  isValidQuery: boolean;
  /**
   * The debounced search query being used
   */
  debouncedQuery: string;
}

/**
 * Hook for searching chat sessions using FTS (Full-Text Search).
 * Automatically debounces search queries and manages search state.
 *
 * Features:
 * - Automatic debouncing to reduce API calls
 * - Minimum query length validation
 * - Scope filtering (sessions, messages, or all)
 * - Category and tag filtering
 * - Results include highlighted snippets and relevance scores
 *
 * @param query - The search query string
 * @param options - Search options and configuration
 * @returns Search results and state
 *
 * @example
 * ```tsx
 * // Basic search
 * const { results, isSearching } = useChatSearch(userInput);
 *
 * // Search with filters
 * const { results } = useChatSearch(userInput, {
 *   scope: 'messages',
 *   category_id: 'work',
 *   limit: 10,
 * });
 *
 * // Custom debounce and validation
 * const { results, isPending } = useChatSearch(userInput, {
 *   debounceDelay: 500,
 *   minLength: 3,
 *   maxLength: 1000,
 *   include_archived: true,
 * });
 * ```
 */
export function useChatSearch(
  query: string,
  options: UseChatSearchOptions = {}
): UseChatSearchReturn {
  const {
    debounceDelay = 300,
    minLength = 2,
    maxLength = 500,
    enabled,
    scope,
    category_id,
    tags,
    include_archived,
    limit,
  } = options;

  // Truncate query to prevent URL length issues
  const trimmedQuery = query.trim();
  const truncatedQuery = trimmedQuery.slice(0, maxLength);

  // Log warning if query was truncated
  if (trimmedQuery.length > maxLength) {
    logger.warn('Search query truncated', {
      originalLength: trimmedQuery.length,
      maxLength,
      component: 'useChatSearch',
    });
  }

  // Debounce the search query to avoid excessive API calls
  const debouncedQuery = useDebounce(truncatedQuery, debounceDelay);

  // Check if query meets minimum length requirement
  const isValidQuery = debouncedQuery.length >= minLength;

  // Determine if the query should be enabled
  const shouldFetch = enabled !== undefined ? enabled : isValidQuery;

  // Build search options (excluding debounce/validation params)
  const searchOptions: Omit<SearchSessionsQuery, 'q'> = useMemo(
    () => ({
      ...(scope && { scope }),
      ...(category_id && { category_id }),
      ...(tags && { tags }),
      ...(include_archived !== undefined && { include_archived }),
      ...(limit && { limit }),
    }),
    [scope, category_id, tags, include_archived, limit]
  );

  // Query for search results
  const {
    data,
    isLoading,
    error,
    isFetching,
  } = useQuery({
    queryKey: QUERY_KEYS.search(debouncedQuery, searchOptions),
    queryFn: async (): Promise<ChatSearchResult[]> => {
      if (!shouldFetch) {
        return [];
      }

      const searchQuery: SearchSessionsQuery = {
        q: debouncedQuery,
        ...searchOptions,
      };

      return await apiClient.searchChatSessions(searchQuery);
    },
    enabled: shouldFetch && debouncedQuery.length > 0,
    // Keep previous results while fetching new ones
    placeholderData: (previousData) => previousData,
    // Cache results for 5 minutes
    staleTime: 5 * 60 * 1000,
    // Refetch on window focus if data is stale
    refetchOnWindowFocus: true,
  });

  // Determine if debounce is pending (compare truncated versions)
  const isPending = truncatedQuery !== debouncedQuery;

  return {
    results: data || [],
    isSearching: isLoading || isFetching,
    isPending,
    error,
    isValidQuery,
    debouncedQuery,
  };
}

/**
 * Hook for searching chat sessions with custom query options.
 * Provides more control over the React Query configuration.
 *
 * @param query - The search query string
 * @param searchOptions - Search parameters (scope, filters, etc.)
 * @param queryOptions - React Query options
 * @param maxLength - Maximum query length (default: 500)
 * @returns React Query result
 *
 * @example
 * ```tsx
 * const { data, isLoading } = useChatSearchQuery(
 *   'project ideas',
 *   { scope: 'all', limit: 50 },
 *   { staleTime: 60000, refetchInterval: 30000 },
 *   1000  // Custom max length
 * );
 * ```
 */
export function useChatSearchQuery(
  query: string,
  searchOptions?: Omit<SearchSessionsQuery, 'q'>,
  queryOptions?: Omit<UseQueryOptions<ChatSearchResult[], Error>, 'queryKey' | 'queryFn'>,
  maxLength: number = 500
) {
  // Truncate query to prevent URL length issues
  const trimmedQuery = query.trim();
  const truncatedQuery = trimmedQuery.slice(0, maxLength);

  // Log warning if query was truncated
  if (trimmedQuery.length > maxLength) {
    logger.warn('Search query truncated', {
      originalLength: trimmedQuery.length,
      maxLength,
      component: 'useChatSearchQuery',
    });
  }

  const debouncedQuery = useDebounce(truncatedQuery, 300);

  return useQuery({
    queryKey: QUERY_KEYS.search(debouncedQuery, searchOptions),
    queryFn: async (): Promise<ChatSearchResult[]> => {
      if (!debouncedQuery) {
        return [];
      }

      const searchQuery: SearchSessionsQuery = {
        q: debouncedQuery,
        ...searchOptions,
      };

      return await apiClient.searchChatSessions(searchQuery);
    },
    enabled: debouncedQuery.length >= 2,
    ...queryOptions,
  });
}

// Export query keys for external cache management
export const chatSearchQueryKeys = QUERY_KEYS;
