import { describe, it, expect, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { usePagination } from '@/hooks/ui/usePagination';

describe('usePagination', () => {
  describe('initialization', () => {
    it('should initialize with default values', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100 })
      );

      expect(result.current.currentPage).toBe(1);
      expect(result.current.pageSize).toBe(10);
      expect(result.current.totalItems).toBe(100);
      expect(result.current.totalPages).toBe(10);
      expect(result.current.startIndex).toBe(0);
      expect(result.current.endIndex).toBe(10);
    });

    it('should initialize with custom values', () => {
      const { result } = renderHook(() =>
        usePagination({
          totalItems: 100,
          pageSize: 25,
          initialPage: 2,
        })
      );

      expect(result.current.currentPage).toBe(2);
      expect(result.current.pageSize).toBe(25);
      expect(result.current.totalPages).toBe(4);
      expect(result.current.startIndex).toBe(25);
      expect(result.current.endIndex).toBe(50);
    });
  });

  describe('computed values', () => {
    it('should calculate totalPages correctly', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 95, pageSize: 10 })
      );

      expect(result.current.totalPages).toBe(10);
    });

    it('should handle exact divisions', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10 })
      );

      expect(result.current.totalPages).toBe(10);
    });

    it('should calculate startIndex and endIndex correctly', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 3 })
      );

      expect(result.current.startIndex).toBe(20);
      expect(result.current.endIndex).toBe(30);
    });

    it('should handle last page endIndex correctly', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 95, pageSize: 10, initialPage: 10 })
      );

      expect(result.current.startIndex).toBe(90);
      expect(result.current.endIndex).toBe(95); // Not 100
    });

    it('should set hasPreviousPage and hasNextPage correctly', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 5 })
      );

      expect(result.current.hasPreviousPage).toBe(true);
      expect(result.current.hasNextPage).toBe(true);
      expect(result.current.isFirstPage).toBe(false);
      expect(result.current.isLastPage).toBe(false);
    });

    it('should handle first page correctly', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 1 })
      );

      expect(result.current.hasPreviousPage).toBe(false);
      expect(result.current.hasNextPage).toBe(true);
      expect(result.current.isFirstPage).toBe(true);
      expect(result.current.isLastPage).toBe(false);
    });

    it('should handle last page correctly', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 10 })
      );

      expect(result.current.hasPreviousPage).toBe(true);
      expect(result.current.hasNextPage).toBe(false);
      expect(result.current.isFirstPage).toBe(false);
      expect(result.current.isLastPage).toBe(true);
    });

    it('should calculate itemRange correctly', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 3 })
      );

      expect(result.current.itemRange).toEqual({
        start: 21,
        end: 30,
        total: 100,
      });
    });

    it('should handle empty list itemRange', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 0, pageSize: 10 })
      );

      expect(result.current.itemRange).toEqual({
        start: 0,
        end: 0,
        total: 0,
      });
    });
  });

  describe('navigation', () => {
    it('should navigate to specific page', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10 })
      );

      act(() => {
        result.current.goToPage(5);
      });

      expect(result.current.currentPage).toBe(5);
      expect(result.current.startIndex).toBe(40);
      expect(result.current.endIndex).toBe(50);
    });

    it('should clamp page to valid range', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10 })
      );

      act(() => {
        result.current.goToPage(15);
      });

      expect(result.current.currentPage).toBe(10); // Clamped to max

      act(() => {
        result.current.goToPage(-5);
      });

      expect(result.current.currentPage).toBe(1); // Clamped to min
    });

    it('should navigate to next page', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 3 })
      );

      act(() => {
        result.current.nextPage();
      });

      expect(result.current.currentPage).toBe(4);
    });

    it('should not navigate past last page', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 10 })
      );

      act(() => {
        result.current.nextPage();
      });

      expect(result.current.currentPage).toBe(10);
    });

    it('should navigate to previous page', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 3 })
      );

      act(() => {
        result.current.previousPage();
      });

      expect(result.current.currentPage).toBe(2);
    });

    it('should not navigate before first page', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 1 })
      );

      act(() => {
        result.current.previousPage();
      });

      expect(result.current.currentPage).toBe(1);
    });

    it('should navigate to first page', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 5 })
      );

      act(() => {
        result.current.firstPage();
      });

      expect(result.current.currentPage).toBe(1);
    });

    it('should navigate to last page', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 3 })
      );

      act(() => {
        result.current.lastPage();
      });

      expect(result.current.currentPage).toBe(10);
    });

    it('should call onPageChange callback', () => {
      const onPageChange = vi.fn();
      const { result } = renderHook(() =>
        usePagination({
          totalItems: 100,
          pageSize: 10,
          onPageChange,
        })
      );

      act(() => {
        result.current.goToPage(5);
      });

      expect(onPageChange).toHaveBeenCalledWith(5);
    });
  });

  describe('page size', () => {
    it('should change page size', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 3 })
      );

      act(() => {
        result.current.setPageSize(25);
      });

      expect(result.current.pageSize).toBe(25);
      expect(result.current.totalPages).toBe(4);
    });

    it('should adjust current page when changing page size', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 5 })
      );

      // On page 5, viewing items 41-50
      expect(result.current.startIndex).toBe(40);

      act(() => {
        result.current.setPageSize(25);
      });

      // Should be on page 2 now (items 26-50)
      expect(result.current.currentPage).toBe(2);
      expect(result.current.startIndex).toBe(25);
    });

    it('should call onPageSizeChange callback', () => {
      const onPageSizeChange = vi.fn();
      const { result } = renderHook(() =>
        usePagination({
          totalItems: 100,
          pageSize: 10,
          onPageSizeChange,
        })
      );

      act(() => {
        result.current.setPageSize(25);
      });

      expect(onPageSizeChange).toHaveBeenCalledWith(25);
    });

    it('should not change page size to invalid values', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10 })
      );

      act(() => {
        result.current.setPageSize(0);
      });

      expect(result.current.pageSize).toBe(10); // Unchanged

      act(() => {
        result.current.setPageSize(-5);
      });

      expect(result.current.pageSize).toBe(10); // Unchanged
    });
  });

  describe('total items', () => {
    it('should update total items', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10 })
      );

      act(() => {
        result.current.setTotalItems(200);
      });

      expect(result.current.totalItems).toBe(200);
      expect(result.current.totalPages).toBe(20);
    });

    it('should adjust current page when total items decrease', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10, initialPage: 10 })
      );

      act(() => {
        result.current.setTotalItems(50);
      });

      expect(result.current.currentPage).toBe(5); // Adjusted to last page
      expect(result.current.totalPages).toBe(5);
    });

    it('should clamp negative total items to 0', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100, pageSize: 10 })
      );

      act(() => {
        result.current.setTotalItems(-10);
      });

      expect(result.current.totalItems).toBe(0);
    });
  });

  describe('reset', () => {
    it('should reset to initial state', () => {
      const { result } = renderHook(() =>
        usePagination({
          totalItems: 100,
          pageSize: 10,
          initialPage: 1,
        })
      );

      // Make changes
      act(() => {
        result.current.goToPage(5);
      });

      expect(result.current.currentPage).toBe(5);

      act(() => {
        result.current.setPageSize(25);
        result.current.setTotalItems(200);
      });

      // setPageSize adjusts the page, so we won't be on page 5 anymore
      expect(result.current.pageSize).toBe(25);
      expect(result.current.totalItems).toBe(200);

      // Reset
      act(() => {
        result.current.reset();
      });

      expect(result.current.currentPage).toBe(1);
      expect(result.current.pageSize).toBe(10);
      expect(result.current.totalItems).toBe(100);
    });
  });

  describe('getPageNumbers', () => {
    it('should return all pages when total is less than maxVisible', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 50, pageSize: 10 })
      );

      const pages = result.current.getPageNumbers(7);
      expect(pages).toEqual([1, 2, 3, 4, 5]);
    });

    it('should return pages with ellipsis for large page count', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 200, pageSize: 10, initialPage: 10 })
      );

      const pages = result.current.getPageNumbers(7);
      expect(pages).toContain('ellipsis');
      expect(pages).toContain(1);
      expect(pages).toContain(20);
    });

    it('should show pages near start', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 200, pageSize: 10, initialPage: 2 })
      );

      const pages = result.current.getPageNumbers(7);
      expect(pages[0]).toBe(1);
      expect(pages[1]).toBe(2);
      expect(pages[pages.length - 1]).toBe(20);
      expect(pages).toContain('ellipsis');
    });

    it('should show pages near end', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 200, pageSize: 10, initialPage: 19 })
      );

      const pages = result.current.getPageNumbers(7);
      expect(pages[0]).toBe(1);
      expect(pages[pages.length - 1]).toBe(20);
      expect(pages).toContain('ellipsis');
    });

    it('should show pages in middle', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 200, pageSize: 10, initialPage: 10 })
      );

      const pages = result.current.getPageNumbers(7);
      expect(pages[0]).toBe(1);
      expect(pages[pages.length - 1]).toBe(20);
      expect(pages).toContain(10);
      // Should have two ellipses for middle position
      expect(pages.filter(p => p === 'ellipsis').length).toBe(2);
    });

    it('should handle custom maxVisible', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 200, pageSize: 10, initialPage: 10 })
      );

      const pages = result.current.getPageNumbers(5);
      // With maxVisible=5 in middle, we get: 1, ellipsis, 9, 10, 11, ellipsis, 20 = 7 items
      // The algorithm adds ellipses and first/last pages
      expect(pages).toContain(1);
      expect(pages).toContain(20);
      expect(pages).toContain(10);
      expect(pages.filter(p => p === 'ellipsis').length).toBe(2);
    });
  });

  describe('edge cases', () => {
    it('should handle single page', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 5, pageSize: 10 })
      );

      expect(result.current.totalPages).toBe(1);
      expect(result.current.hasPreviousPage).toBe(false);
      expect(result.current.hasNextPage).toBe(false);
      expect(result.current.isFirstPage).toBe(true);
      expect(result.current.isLastPage).toBe(true);
    });

    it('should handle zero items', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 0, pageSize: 10 })
      );

      expect(result.current.totalPages).toBe(1);
      expect(result.current.startIndex).toBe(0);
      expect(result.current.endIndex).toBe(0);
    });

    it('should return default page size options', () => {
      const { result } = renderHook(() =>
        usePagination({ totalItems: 100 })
      );

      expect(result.current.pageSizeOptions).toEqual([10, 25, 50, 100]);
    });

    it('should use custom page size options', () => {
      const { result } = renderHook(() =>
        usePagination({
          totalItems: 100,
          pageSizeOptions: [5, 20, 50],
        })
      );

      expect(result.current.pageSizeOptions).toEqual([5, 20, 50]);
    });
  });
});
