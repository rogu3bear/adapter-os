/**
 * Tests for DatasetChatContext
 *
 * Tests context provider, state management, and sessionStorage persistence.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { ReactNode } from 'react';
import {
  DatasetChatProvider,
  useDatasetChat,
  useDatasetChatOptional,
} from '@/contexts/DatasetChatContext';

const SESSION_KEYS = {
  ACTIVE_DATASET_ID: 'datasetChat:activeDatasetId',
  ACTIVE_DATASET_NAME: 'datasetChat:activeDatasetName',
  COLLECTION_ID: 'datasetChat:collectionId',
  VERSION_ID: 'datasetChat:datasetVersionId',
} as const;

function createWrapper(initialDataset?: {
  id: string;
  name: string;
  collectionId?: string;
  versionId?: string;
}) {
  return ({ children }: { children: ReactNode }) => (
    <DatasetChatProvider initialDataset={initialDataset}>
      {children}
    </DatasetChatProvider>
  );
}

describe('DatasetChatContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    sessionStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Provider and Hook', () => {
    it('throws error when useDatasetChat is used outside provider', () => {
      // Suppress expected console.error
      const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

      expect(() => {
        renderHook(() => useDatasetChat());
      }).toThrow('useDatasetChat must be used within a DatasetChatProvider');

      consoleError.mockRestore();
    });

    it('returns null when useDatasetChatOptional is used outside provider', () => {
      const { result } = renderHook(() => useDatasetChatOptional());
      expect(result.current).toBeNull();
    });

    it('provides context value when used within provider', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      expect(result.current).toBeDefined();
      expect(result.current.activeDatasetId).toBeNull();
      expect(result.current.activeDatasetName).toBeNull();
    });
  });

  describe('Initialization', () => {
    it('initializes with null values when no initial dataset provided', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      expect(result.current.activeDatasetId).toBeNull();
      expect(result.current.activeDatasetName).toBeNull();
      expect(result.current.collectionId).toBeNull();
      expect(result.current.datasetVersionId).toBeNull();
    });

    it('initializes with provided initial dataset', () => {
      const initialDataset = {
        id: 'dataset-123',
        name: 'Test Dataset',
        collectionId: 'col-456',
        versionId: 'v1.0',
      };

      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(initialDataset),
      });

      expect(result.current.activeDatasetId).toBe('dataset-123');
      expect(result.current.activeDatasetName).toBe('Test Dataset');
      expect(result.current.collectionId).toBe('col-456');
      expect(result.current.datasetVersionId).toBe('v1.0');
    });

    it('initializes from sessionStorage when no initial dataset provided', () => {
      sessionStorage.setItem(SESSION_KEYS.ACTIVE_DATASET_ID, 'dataset-stored');
      sessionStorage.setItem(SESSION_KEYS.ACTIVE_DATASET_NAME, 'Stored Dataset');
      sessionStorage.setItem(SESSION_KEYS.COLLECTION_ID, 'col-stored');
      sessionStorage.setItem(SESSION_KEYS.VERSION_ID, 'v2.0');

      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      expect(result.current.activeDatasetId).toBe('dataset-stored');
      expect(result.current.activeDatasetName).toBe('Stored Dataset');
      expect(result.current.collectionId).toBe('col-stored');
      expect(result.current.datasetVersionId).toBe('v2.0');
    });

    it('prioritizes initial dataset over sessionStorage', () => {
      sessionStorage.setItem(SESSION_KEYS.ACTIVE_DATASET_ID, 'dataset-stored');
      sessionStorage.setItem(SESSION_KEYS.ACTIVE_DATASET_NAME, 'Stored Dataset');

      const initialDataset = {
        id: 'dataset-initial',
        name: 'Initial Dataset',
      };

      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(initialDataset),
      });

      expect(result.current.activeDatasetId).toBe('dataset-initial');
      expect(result.current.activeDatasetName).toBe('Initial Dataset');
    });
  });

  describe('setActiveDataset', () => {
    it('sets active dataset with all fields', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-abc',
          name: 'ABC Dataset',
          collectionId: 'col-xyz',
          versionId: 'v3.0',
        });
      });

      expect(result.current.activeDatasetId).toBe('dataset-abc');
      expect(result.current.activeDatasetName).toBe('ABC Dataset');
      expect(result.current.collectionId).toBe('col-xyz');
      expect(result.current.datasetVersionId).toBe('v3.0');
    });

    it('sets active dataset with minimal fields', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-min',
          name: 'Minimal Dataset',
        });
      });

      expect(result.current.activeDatasetId).toBe('dataset-min');
      expect(result.current.activeDatasetName).toBe('Minimal Dataset');
      expect(result.current.collectionId).toBeNull();
      expect(result.current.datasetVersionId).toBeNull();
    });

    it('persists to sessionStorage when dataset is set', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-persist',
          name: 'Persist Dataset',
          collectionId: 'col-persist',
          versionId: 'v1.5',
        });
      });

      expect(sessionStorage.getItem(SESSION_KEYS.ACTIVE_DATASET_ID)).toBe('dataset-persist');
      expect(sessionStorage.getItem(SESSION_KEYS.ACTIVE_DATASET_NAME)).toBe('Persist Dataset');
      expect(sessionStorage.getItem(SESSION_KEYS.COLLECTION_ID)).toBe('col-persist');
      expect(sessionStorage.getItem(SESSION_KEYS.VERSION_ID)).toBe('v1.5');
    });

    it('overwrites previous dataset', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-1',
          name: 'Dataset One',
        });
      });

      expect(result.current.activeDatasetId).toBe('dataset-1');

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-2',
          name: 'Dataset Two',
        });
      });

      expect(result.current.activeDatasetId).toBe('dataset-2');
      expect(result.current.activeDatasetName).toBe('Dataset Two');
    });
  });

  describe('clearActiveDataset', () => {
    it('clears all dataset state', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-clear',
          name: 'Clear Me',
          collectionId: 'col-clear',
          versionId: 'v1.0',
        });
      });

      expect(result.current.activeDatasetId).toBe('dataset-clear');

      act(() => {
        result.current.clearActiveDataset();
      });

      expect(result.current.activeDatasetId).toBeNull();
      expect(result.current.activeDatasetName).toBeNull();
      expect(result.current.collectionId).toBeNull();
      expect(result.current.datasetVersionId).toBeNull();
    });

    it('clears sessionStorage when dataset is cleared', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-clear',
          name: 'Clear Me',
        });
      });

      expect(sessionStorage.getItem(SESSION_KEYS.ACTIVE_DATASET_ID)).toBe('dataset-clear');

      act(() => {
        result.current.clearActiveDataset();
      });

      expect(sessionStorage.getItem(SESSION_KEYS.ACTIVE_DATASET_ID)).toBeNull();
      expect(sessionStorage.getItem(SESSION_KEYS.ACTIVE_DATASET_NAME)).toBeNull();
    });

    it('can clear and set dataset again', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-1',
          name: 'Dataset One',
        });
      });

      act(() => {
        result.current.clearActiveDataset();
      });

      expect(result.current.activeDatasetId).toBeNull();

      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-2',
          name: 'Dataset Two',
        });
      });

      expect(result.current.activeDatasetId).toBe('dataset-2');
    });
  });

  describe('SessionStorage Persistence', () => {
    it('persists state across remounts', () => {
      const { result: result1, unmount } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      act(() => {
        result1.current.setActiveDataset({
          id: 'dataset-persist',
          name: 'Persist Dataset',
          collectionId: 'col-123',
        });
      });

      unmount();

      // Create new hook instance with same wrapper
      const { result: result2 } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      expect(result2.current.activeDatasetId).toBe('dataset-persist');
      expect(result2.current.activeDatasetName).toBe('Persist Dataset');
      expect(result2.current.collectionId).toBe('col-123');
    });

    it('handles missing sessionStorage gracefully', () => {
      // Mock sessionStorage to throw error
      const getItemSpy = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
        throw new Error('Storage access denied');
      });

      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      expect(result.current.activeDatasetId).toBeNull();

      getItemSpy.mockRestore();
    });

    it('handles sessionStorage write failures gracefully', () => {
      const setItemSpy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
        throw new Error('Storage quota exceeded');
      });

      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      // Should not throw error
      act(() => {
        result.current.setActiveDataset({
          id: 'dataset-fail',
          name: 'Fail Dataset',
        });
      });

      // State should still be updated in memory
      expect(result.current.activeDatasetId).toBe('dataset-fail');

      setItemSpy.mockRestore();
    });
  });

  describe('Integration Tests', () => {
    it('supports multiple dataset switches in sequence', () => {
      const { result } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      const datasets = [
        { id: 'ds1', name: 'Dataset 1', versionId: 'v1' },
        { id: 'ds2', name: 'Dataset 2', versionId: 'v2' },
        { id: 'ds3', name: 'Dataset 3', versionId: 'v3' },
      ];

      datasets.forEach((dataset) => {
        act(() => {
          result.current.setActiveDataset(dataset);
        });

        expect(result.current.activeDatasetId).toBe(dataset.id);
        expect(result.current.activeDatasetName).toBe(dataset.name);
        expect(result.current.datasetVersionId).toBe(dataset.versionId);
      });
    });

    it('maintains referential stability of callback functions', () => {
      const { result, rerender } = renderHook(() => useDatasetChat(), {
        wrapper: createWrapper(),
      });

      const setActiveDataset1 = result.current.setActiveDataset;
      const clearActiveDataset1 = result.current.clearActiveDataset;

      rerender();

      const setActiveDataset2 = result.current.setActiveDataset;
      const clearActiveDataset2 = result.current.clearActiveDataset;

      expect(setActiveDataset1).toBe(setActiveDataset2);
      expect(clearActiveDataset1).toBe(clearActiveDataset2);
    });
  });
});
