/**
 * Pagination State Hook
 *
 * Manages pagination state for lists and tables.
 * Supports page-based and cursor-based pagination.
 *
 * Usage:
 * ```tsx
 * const pagination = usePagination({ totalItems: 100, pageSize: 10 });
 *
 * // In your component
 * <Table data={items.slice(pagination.startIndex, pagination.endIndex)} />
 * <Pagination
 *   currentPage={pagination.currentPage}
 *   totalPages={pagination.totalPages}
 *   onPageChange={pagination.goToPage}
 * />
 * ```
 *
 * Citations:
 * - docs/UI_INTEGRATION.md - Table pagination patterns
 */

import { useState, useCallback, useMemo, useEffect } from 'react';

export interface PaginationOptions {
  /** Total number of items */
  totalItems: number;
  /** Items per page (default: 10) */
  pageSize?: number;
  /** Initial page (1-indexed, default: 1) */
  initialPage?: number;
  /** Available page size options */
  pageSizeOptions?: number[];
  /** Callback when page changes */
  onPageChange?: (page: number) => void;
  /** Callback when page size changes */
  onPageSizeChange?: (pageSize: number) => void;
}

export interface PaginationState {
  /** Current page (1-indexed) */
  currentPage: number;
  /** Items per page */
  pageSize: number;
  /** Total number of items */
  totalItems: number;
  /** Total number of pages */
  totalPages: number;
  /** Start index for current page (0-indexed) */
  startIndex: number;
  /** End index for current page (exclusive) */
  endIndex: number;
  /** Whether there's a previous page */
  hasPreviousPage: boolean;
  /** Whether there's a next page */
  hasNextPage: boolean;
  /** Whether currently on first page */
  isFirstPage: boolean;
  /** Whether currently on last page */
  isLastPage: boolean;
  /** Range of items shown (e.g., "1-10 of 100") */
  itemRange: { start: number; end: number; total: number };
}

export interface UsePaginationReturn extends PaginationState {
  /** Go to specific page */
  goToPage: (page: number) => void;
  /** Go to next page */
  nextPage: () => void;
  /** Go to previous page */
  previousPage: () => void;
  /** Go to first page */
  firstPage: () => void;
  /** Go to last page */
  lastPage: () => void;
  /** Change page size */
  setPageSize: (size: number) => void;
  /** Update total items count */
  setTotalItems: (total: number) => void;
  /** Reset to initial state */
  reset: () => void;
  /** Available page size options */
  pageSizeOptions: number[];
  /** Generate array of page numbers for pagination UI */
  getPageNumbers: (maxVisible?: number) => (number | 'ellipsis')[];
}

const DEFAULT_PAGE_SIZE = 10;
const DEFAULT_PAGE_SIZE_OPTIONS = [10, 25, 50, 100];

/**
 * Hook for managing pagination state.
 *
 * @param options - Pagination configuration options
 * @returns Pagination state and control functions
 */
export function usePagination(options: PaginationOptions): UsePaginationReturn {
  const {
    totalItems: initialTotalItems,
    pageSize: initialPageSize = DEFAULT_PAGE_SIZE,
    initialPage = 1,
    pageSizeOptions = DEFAULT_PAGE_SIZE_OPTIONS,
    onPageChange,
    onPageSizeChange,
  } = options;

  const [currentPage, setCurrentPage] = useState(initialPage);
  const [pageSize, setPageSizeState] = useState(initialPageSize);
  const [totalItems, setTotalItemsState] = useState(initialTotalItems);

  // Calculate derived values
  const totalPages = useMemo(
    () => Math.max(1, Math.ceil(totalItems / pageSize)),
    [totalItems, pageSize]
  );

  // Ensure current page is within bounds when totalPages changes
  useEffect(() => {
    if (currentPage > totalPages) {
      setCurrentPage(totalPages);
    }
  }, [currentPage, totalPages]);

  const startIndex = useMemo(
    () => (currentPage - 1) * pageSize,
    [currentPage, pageSize]
  );

  const endIndex = useMemo(
    () => Math.min(startIndex + pageSize, totalItems),
    [startIndex, pageSize, totalItems]
  );

  const hasPreviousPage = currentPage > 1;
  const hasNextPage = currentPage < totalPages;
  const isFirstPage = currentPage === 1;
  const isLastPage = currentPage === totalPages;

  const itemRange = useMemo(
    () => ({
      start: totalItems > 0 ? startIndex + 1 : 0,
      end: endIndex,
      total: totalItems,
    }),
    [startIndex, endIndex, totalItems]
  );

  const goToPage = useCallback(
    (page: number) => {
      const validPage = Math.max(1, Math.min(page, totalPages));
      if (validPage !== currentPage) {
        setCurrentPage(validPage);
        onPageChange?.(validPage);
      }
    },
    [totalPages, currentPage, onPageChange]
  );

  const nextPage = useCallback(() => {
    if (hasNextPage) {
      goToPage(currentPage + 1);
    }
  }, [hasNextPage, currentPage, goToPage]);

  const previousPage = useCallback(() => {
    if (hasPreviousPage) {
      goToPage(currentPage - 1);
    }
  }, [hasPreviousPage, currentPage, goToPage]);

  const firstPage = useCallback(() => {
    goToPage(1);
  }, [goToPage]);

  const lastPage = useCallback(() => {
    goToPage(totalPages);
  }, [goToPage, totalPages]);

  const setPageSize = useCallback(
    (size: number) => {
      if (size !== pageSize && size > 0) {
        // Adjust current page to keep approximately the same items visible
        const currentFirstItem = startIndex + 1;
        const newPage = Math.ceil(currentFirstItem / size);

        setPageSizeState(size);
        setCurrentPage(newPage);
        onPageSizeChange?.(size);
      }
    },
    [pageSize, startIndex, onPageSizeChange]
  );

  const setTotalItems = useCallback((total: number) => {
    setTotalItemsState(Math.max(0, total));
  }, []);

  const reset = useCallback(() => {
    setCurrentPage(initialPage);
    setPageSizeState(initialPageSize);
    setTotalItemsState(initialTotalItems);
  }, [initialPage, initialPageSize, initialTotalItems]);

  const getPageNumbers = useCallback(
    (maxVisible: number = 7): (number | 'ellipsis')[] => {
      if (totalPages <= maxVisible) {
        return Array.from({ length: totalPages }, (_, i) => i + 1);
      }

      const pages: (number | 'ellipsis')[] = [];
      const sidePages = Math.floor((maxVisible - 3) / 2);

      // Always show first page
      pages.push(1);

      if (currentPage <= sidePages + 2) {
        // Near the start
        for (let i = 2; i <= Math.min(maxVisible - 2, totalPages - 1); i++) {
          pages.push(i);
        }
        if (totalPages > maxVisible - 1) {
          pages.push('ellipsis');
        }
      } else if (currentPage >= totalPages - sidePages - 1) {
        // Near the end
        pages.push('ellipsis');
        for (let i = Math.max(totalPages - maxVisible + 3, 2); i <= totalPages - 1; i++) {
          pages.push(i);
        }
      } else {
        // In the middle
        pages.push('ellipsis');
        for (let i = currentPage - sidePages; i <= currentPage + sidePages; i++) {
          pages.push(i);
        }
        pages.push('ellipsis');
      }

      // Always show last page
      if (totalPages > 1) {
        pages.push(totalPages);
      }

      return pages;
    },
    [totalPages, currentPage]
  );

  return {
    currentPage,
    pageSize,
    totalItems,
    totalPages,
    startIndex,
    endIndex,
    hasPreviousPage,
    hasNextPage,
    isFirstPage,
    isLastPage,
    itemRange,
    goToPage,
    nextPage,
    previousPage,
    firstPage,
    lastPage,
    setPageSize,
    setTotalItems,
    reset,
    pageSizeOptions,
    getPageNumbers,
  };
}

export default usePagination;
