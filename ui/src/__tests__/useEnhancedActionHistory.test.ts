//! Tests for Enhanced Action History Hook
//!
//! Demonstrates usage patterns and validates core functionality.

import { renderHook, act } from '@testing-library/react';
import useEnhancedActionHistory from '@/hooks/useEnhancedActionHistory';
import { ActionHistoryItem } from '@/types/history';

describe('useEnhancedActionHistory', () => {
  beforeEach(() => {
    // Clear localStorage to isolate tests
    localStorage.clear();
  });
  it('should add action to history', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Created test adapter',
        undo: async () => {},
        redo: async () => {},
      });
    });

    expect(result.current.historyCount).toBe(1);
    expect(result.current.allActions[0].action).toBe('create');
  });

  it('should support undo and redo', async () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    let undoCalled = false;
    let redoCalled = false;

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Test action',
        undo: async () => { undoCalled = true; },
        redo: async () => { redoCalled = true; },
      });
    });

    expect(result.current.canUndo).toBe(true);
    expect(result.current.canRedo).toBe(false);

    await act(async () => {
      await result.current.undo();
    });

    expect(undoCalled).toBe(true);
    expect(result.current.canRedo).toBe(true);
  });

  it('should filter actions by type', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Create action',
        undo: async () => {},
      });

      result.current.addAction({
        action: 'delete',
        resource: 'adapter',
        status: 'success',
        description: 'Delete action',
        undo: async () => {},
      });
    });

    act(() => {
      result.current.setFilter({ actionTypes: ['create'] });
    });

    expect(result.current.filteredActions.length).toBe(1);
    expect(result.current.filteredActions[0].action).toBe('create');
  });

  it('should filter actions by status', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Success',
        undo: async () => {},
      });

      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'failed',
        description: 'Failed',
        undo: async () => {},
      });
    });

    act(() => {
      result.current.setFilter({ statuses: ['failed'] });
    });

    expect(result.current.filteredActions.length).toBe(1);
    expect(result.current.filteredActions[0].status).toBe('failed');
  });

  it('should search in description', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Created adapter: my-adapter',
        undo: async () => {},
      });

      result.current.addAction({
        action: 'delete',
        resource: 'adapter',
        status: 'success',
        description: 'Deleted something else',
        undo: async () => {},
      });
    });

    act(() => {
      result.current.setSearch('my-adapter');
    });

    expect(result.current.filteredActions.length).toBe(1);
    expect(result.current.filteredActions[0].description).toContain('my-adapter');
  });

  it('should support pagination', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    // Add 10 actions
    act(() => {
      for (let i = 0; i < 10; i++) {
        result.current.addAction({
          action: 'create',
          resource: 'adapter',
          status: 'success',
          description: `Action ${i}`,
          undo: async () => {},
        });
      }
    });

    // Verify all actions were added
    expect(result.current.allActions.length).toBe(10);

    // Trigger filter recalculation by setting empty filter
    act(() => {
      result.current.setFilter({});
    });

    act(() => {
      result.current.setPagination({ page: 0, pageSize: 5 });
    });

    expect(result.current.paginatedActions.length).toBe(5);
    expect(result.current.totalPages).toBe(2);

    act(() => {
      result.current.setPagination({ page: 1, pageSize: 5 });
    });

    expect(result.current.paginatedActions.length).toBe(5);
  });

  it('should handle action selection', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Test',
        undo: async () => {},
      });
    });

    // Get actionId after state has updated
    const actionId = result.current.allActions[0].id;

    act(() => {
      result.current.toggleSelection(actionId);
    });

    expect(result.current.selectedCount).toBe(1);
    expect(result.current.isSelected(actionId)).toBe(true);

    act(() => {
      result.current.clearSelection();
    });

    expect(result.current.selectedCount).toBe(0);
  });

  it('should export to JSON', async () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Test action',
        undo: async () => {},
        metadata: { test: true },
      });
    });

    const json = await result.current.exportHistory({
      format: 'json',
      scope: 'all',
      includeMetadata: true,
    });

    const parsed = JSON.parse(json);
    expect(parsed).toBeInstanceOf(Array);
    expect(parsed[0].description).toBe('Test action');
    expect(parsed[0].metadata?.test).toBe(true);
  });

  it('should export to CSV', async () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Test action',
        undo: async () => {},
      });
    });

    const csv = await result.current.exportHistory({
      format: 'csv',
      scope: 'all',
    });

    expect(csv).toContain('create');
    expect(csv).toContain('adapter');
    expect(csv).toContain('Test action');
  });

  it('should calculate statistics', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Success',
        duration: 100,
        undo: async () => {},
      });

      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'failed',
        description: 'Failed',
        duration: 50,
        undo: async () => {},
      });

      result.current.addAction({
        action: 'delete',
        resource: 'adapter',
        status: 'success',
        description: 'Delete',
        duration: 75,
        undo: async () => {},
      });
    });

    const stats = result.current.stats;
    expect(stats.totalActions).toBe(3);
    expect(stats.successRate).toBe(66.66666666666666);
    expect(stats.actionsByType.create).toBe(2);
    expect(stats.actionsByType.delete).toBe(1);
    expect(stats.averageDuration).toBeCloseTo(75, 0);
  });

  it('should limit history size', () => {
    const { result } = renderHook(() => useEnhancedActionHistory({ maxSize: 5 }));

    act(() => {
      for (let i = 0; i < 10; i++) {
        result.current.addAction({
          action: 'create',
          resource: 'adapter',
          status: 'success',
          description: `Action ${i}`,
          undo: async () => {},
        });
      }
    });

    expect(result.current.historyCount).toBe(5);
  });

  it('should clear history', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Test',
        undo: async () => {},
      });
    });

    expect(result.current.historyCount).toBe(1);

    act(() => {
      result.current.clearHistory();
    });

    expect(result.current.historyCount).toBe(0);
  });

  it('should get action by ID', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      result.current.addAction({
        action: 'create',
        resource: 'adapter',
        status: 'success',
        description: 'Test',
        undo: async () => {},
      });
    });

    // Get actionId after state has updated
    const actionId = result.current.allActions[0].id;

    const action = result.current.getActionById(actionId);
    expect(action).toBeDefined();
    expect(action?.description).toBe('Test');
  });

  it('should handle complex filter combinations', () => {
    const { result } = renderHook(() => useEnhancedActionHistory());

    act(() => {
      const now = Date.now();

      for (let i = 0; i < 5; i++) {
        result.current.addAction({
          action: i % 2 === 0 ? 'create' : 'delete',
          resource: 'adapter',
          status: i < 3 ? 'success' : 'failed',
          description: `Action ${i}`,
          timestamp: now - (i * 1000),
          undo: async () => {},
          tags: i < 2 ? ['production'] : ['test'],
        });
      }
    });

    // Complex filter
    act(() => {
      result.current.setFilter({
        actionTypes: ['create'],
        statuses: ['success'],
        tags: ['production'],
      });
    });

    expect(result.current.filteredActions.length).toBeGreaterThan(0);
    result.current.filteredActions.forEach((action) => {
      expect(action.action).toBe('create');
      expect(action.status).toBe('success');
      expect(action.tags).toContain('production');
    });
  });
});
