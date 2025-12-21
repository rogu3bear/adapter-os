import { describe, it, expect, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useSort } from '@/hooks/ui/useSort';

interface TestData {
  id: number;
  name: string;
  age: number;
  active: boolean;
  created: Date;
}

describe('useSort', () => {
  const mockData: TestData[] = [
    { id: 1, name: 'Alice', age: 30, active: true, created: new Date('2024-01-01') },
    { id: 2, name: 'Bob', age: 25, active: false, created: new Date('2024-02-01') },
    { id: 3, name: 'Charlie', age: 35, active: true, created: new Date('2024-03-01') },
    { id: 4, name: 'David', age: 28, active: false, created: new Date('2024-04-01') },
    { id: 5, name: 'Eve', age: 32, active: true, created: new Date('2024-05-01') },
  ];

  describe('initialization', () => {
    it('should initialize without sorting', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      expect(result.current.sortConfig).toBeNull();
      expect(result.current.sortedData).toEqual(mockData);
    });

    it('should initialize with default sort', () => {
      const { result } = renderHook(() =>
        useSort({
          data: mockData,
          defaultSort: { key: 'name', direction: 'asc' },
        })
      );

      expect(result.current.sortConfig).toEqual({
        key: 'name',
        direction: 'asc',
      });
      expect(result.current.sortedData[0].name).toBe('Alice');
      expect(result.current.sortedData[4].name).toBe('Eve');
    });
  });

  describe('single column sorting', () => {
    it('should sort by string field ascending', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.sortConfig?.direction).toBe('asc');
      expect(result.current.sortedData[0].name).toBe('Alice');
      expect(result.current.sortedData[4].name).toBe('Eve');
    });

    it('should sort by string field descending', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      act(() => {
        result.current.handleSort('name');
      });

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.sortConfig?.direction).toBe('desc');
      expect(result.current.sortedData[0].name).toBe('Eve');
      expect(result.current.sortedData[4].name).toBe('Alice');
    });

    it('should clear sort on third click', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      act(() => {
        result.current.handleSort('name');
      });

      act(() => {
        result.current.handleSort('name');
      });

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.sortConfig).toBeNull();
      expect(result.current.sortedData).toEqual(mockData);
    });

    it('should sort by number field', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      act(() => {
        result.current.handleSort('age');
      });

      expect(result.current.sortedData[0].age).toBe(25);
      expect(result.current.sortedData[4].age).toBe(35);
    });

    it('should sort by boolean field', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      act(() => {
        result.current.handleSort('active');
      });

      // true comes before false in boolean sorting
      expect(result.current.sortedData[0].active).toBe(true);
      expect(result.current.sortedData[mockData.filter(d => d.active).length].active).toBe(false);
    });

    it('should sort by date field', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      act(() => {
        result.current.handleSort('created');
      });

      expect(result.current.sortedData[0].created).toEqual(new Date('2024-01-01'));
      expect(result.current.sortedData[4].created).toEqual(new Date('2024-05-01'));
    });

    it('should switch to different column', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.sortConfig?.key).toBe('name');

      act(() => {
        result.current.handleSort('age');
      });

      expect(result.current.sortConfig?.key).toBe('age');
      expect(result.current.sortConfig?.direction).toBe('asc');
    });
  });

  describe('multi-column sorting', () => {
    it('should enable multi-column sorting', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData, multiSort: true })
      );

      act(() => {
        result.current.handleSort('active');
      });

      act(() => {
        result.current.handleSort('age');
      });

      expect(result.current.multiSortConfig).toHaveLength(2);
      expect(result.current.multiSortConfig[0].key).toBe('active');
      expect(result.current.multiSortConfig[1].key).toBe('age');
    });

    it('should respect maxMultiSortColumns', () => {
      const { result } = renderHook(() =>
        useSort({
          data: mockData,
          multiSort: true,
          maxMultiSortColumns: 2,
        })
      );

      act(() => {
        result.current.handleSort('name');
      });

      act(() => {
        result.current.handleSort('age');
      });

      act(() => {
        result.current.handleSort('active');
      });

      expect(result.current.multiSortConfig).toHaveLength(2);
      expect(result.current.multiSortConfig[0].key).toBe('age');
      expect(result.current.multiSortConfig[1].key).toBe('active');
    });

    it('should toggle direction in multi-sort', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData, multiSort: true })
      );

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.multiSortConfig[0].direction).toBe('asc');

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.multiSortConfig[0].direction).toBe('desc');
    });

    it('should remove sort column on third click in multi-sort', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData, multiSort: true })
      );

      act(() => {
        result.current.handleSort('name');
      });

      act(() => {
        result.current.handleSort('age');
      });

      expect(result.current.multiSortConfig).toHaveLength(2);

      act(() => {
        result.current.handleSort('name');
      });

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.multiSortConfig).toHaveLength(1);
      expect(result.current.multiSortConfig[0].key).toBe('age');
    });
  });

  describe('custom comparators', () => {
    it('should use custom comparator', () => {
      const customComparator = (a: TestData, b: TestData) => {
        // Custom sort: even ages before odd ages
        const aEven = a.age % 2 === 0;
        const bEven = b.age % 2 === 0;
        if (aEven && !bEven) return -1;
        if (!aEven && bEven) return 1;
        return a.age - b.age;
      };

      const { result } = renderHook(() =>
        useSort({
          data: mockData,
          comparators: { age: customComparator },
        })
      );

      act(() => {
        result.current.handleSort('age');
      });

      // Even ages should come first
      expect(result.current.sortedData[0].age % 2).toBe(0);
      expect(result.current.sortedData[1].age % 2).toBe(0);
    });
  });

  describe('helper functions', () => {
    it('should check if column is sorted', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      expect(result.current.isSorted('name')).toBe(false);

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.isSorted('name')).toBe(true);
      expect(result.current.isSorted('age')).toBe(false);
    });

    it('should get sort direction', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      expect(result.current.getSortDirection('name')).toBeNull();

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.getSortDirection('name')).toBe('asc');

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.getSortDirection('name')).toBe('desc');
    });

    it('should get sort priority in multi-sort', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData, multiSort: true })
      );

      act(() => {
        result.current.handleSort('name');
      });

      act(() => {
        result.current.handleSort('age');
      });

      expect(result.current.getSortPriority('name')).toBe(1);
      expect(result.current.getSortPriority('age')).toBe(2);
      expect(result.current.getSortPriority('active')).toBeNull();
    });
  });

  describe('direct control', () => {
    it('should set sort configuration directly', () => {
      const { result } = renderHook(() =>
        useSort({ data: mockData })
      );

      act(() => {
        result.current.setSort({ key: 'age', direction: 'desc' });
      });

      expect(result.current.sortConfig).toEqual({
        key: 'age',
        direction: 'desc',
      });
      expect(result.current.sortedData[0].age).toBe(35);
    });

    it('should clear sort', () => {
      const { result } = renderHook(() =>
        useSort({
          data: mockData,
          defaultSort: { key: 'name', direction: 'asc' },
        })
      );

      expect(result.current.sortConfig).not.toBeNull();

      act(() => {
        result.current.clearSort();
      });

      expect(result.current.sortConfig).toBeNull();
      expect(result.current.sortedData).toEqual(mockData);
    });

    it('should toggle direction', () => {
      const { result } = renderHook(() =>
        useSort({
          data: mockData,
          defaultSort: { key: 'name', direction: 'asc' },
        })
      );

      expect(result.current.sortConfig?.direction).toBe('asc');

      act(() => {
        result.current.toggleDirection();
      });

      expect(result.current.sortConfig?.direction).toBe('desc');

      act(() => {
        result.current.toggleDirection();
      });

      expect(result.current.sortConfig?.direction).toBe('asc');
    });
  });

  describe('callbacks', () => {
    it('should call onSortChange when sort changes', () => {
      const onSortChange = vi.fn();
      const { result } = renderHook(() =>
        useSort({
          data: mockData,
          onSortChange,
        })
      );

      act(() => {
        result.current.handleSort('name');
      });

      expect(onSortChange).toHaveBeenCalledWith({
        key: 'name',
        direction: 'asc',
      });
    });

    it('should call onSortChange when using setSort', () => {
      const onSortChange = vi.fn();
      const { result } = renderHook(() =>
        useSort({
          data: mockData,
          onSortChange,
        })
      );

      act(() => {
        result.current.setSort({ key: 'age', direction: 'desc' });
      });

      expect(onSortChange).toHaveBeenCalledWith({
        key: 'age',
        direction: 'desc',
      });
    });

    it('should call onSortChange with array in multi-sort mode', () => {
      const onSortChange = vi.fn();
      const { result } = renderHook(() =>
        useSort({
          data: mockData,
          multiSort: true,
          onSortChange,
        })
      );

      act(() => {
        result.current.handleSort('name');
      });

      expect(onSortChange).toHaveBeenCalledWith([
        { key: 'name', direction: 'asc' },
      ]);
    });
  });

  describe('edge cases', () => {
    it('should handle empty data', () => {
      const { result } = renderHook(() =>
        useSort({ data: [] })
      );

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.sortedData).toEqual([]);
    });

    it('should handle null values', () => {
      const dataWithNull = [
        { id: 1, name: 'Alice', value: 10 },
        { id: 2, name: null, value: 20 },
        { id: 3, name: 'Charlie', value: null },
      ] as any[];

      const { result } = renderHook(() =>
        useSort({ data: dataWithNull })
      );

      act(() => {
        result.current.handleSort('name');
      });

      // null should be sorted to the end
      expect(result.current.sortedData[2].name).toBeNull();

      act(() => {
        result.current.handleSort('value');
      });

      expect(result.current.sortedData[2].value).toBeNull();
    });

    it('should handle single item', () => {
      const singleItem = [mockData[0]];
      const { result } = renderHook(() =>
        useSort({ data: singleItem })
      );

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.sortedData).toEqual(singleItem);
    });

    it('should handle identical values', () => {
      const identicalData = [
        { id: 1, name: 'Same', value: 10 },
        { id: 2, name: 'Same', value: 10 },
        { id: 3, name: 'Same', value: 10 },
      ];

      const { result } = renderHook(() =>
        useSort({ data: identicalData })
      );

      act(() => {
        result.current.handleSort('name');
      });

      expect(result.current.sortedData).toHaveLength(3);
    });

    it('should handle date strings', () => {
      const dateStringData = [
        { id: 1, date: '2024-03-01' },
        { id: 2, date: '2024-01-01' },
        { id: 3, date: '2024-02-01' },
      ];

      const { result } = renderHook(() =>
        useSort({ data: dateStringData })
      );

      act(() => {
        result.current.handleSort('date');
      });

      expect(result.current.sortedData[0].date).toBe('2024-01-01');
      expect(result.current.sortedData[2].date).toBe('2024-03-01');
    });
  });

  describe('reactivity', () => {
    it('should re-sort when data changes', () => {
      const { result, rerender } = renderHook(
        ({ data }) => useSort({ data }),
        { initialProps: { data: mockData } }
      );

      act(() => {
        result.current.handleSort('name');
      });

      const newData = [...mockData, { id: 6, name: 'Zara', age: 40, active: true, created: new Date() }];
      rerender({ data: newData });

      expect(result.current.sortedData).toHaveLength(6);
      expect(result.current.sortedData[5].name).toBe('Zara');
    });
  });
});
