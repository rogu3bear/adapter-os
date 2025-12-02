/**
 * Unit Tests for useModelLoadingState Hook
 *
 * Tests the unified model + adapter readiness tracking hook.
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useModelLoadingState } from '@/hooks/model-loading/useModelLoadingState';
import type { ModelStatusState } from '@/hooks/model-loading/types';

// Mock dependencies
const mockModelStatus = {
  status: 'loaded' as ModelStatusState,
  isReady: true,
  modelId: 'test-model-id',
  modelName: 'Test Model',
  memoryUsageMb: 4096,
  errorMessage: null,
  refresh: vi.fn().mockResolvedValue(undefined),
};

const mockAdapterState = {
  adapterStates: new Map([
    ['adapter-1', {
      adapterId: 'adapter-1',
      name: 'Adapter 1',
      state: 'warm',
      isLoading: false,
      error: undefined,
    }],
    ['adapter-2', {
      adapterId: 'adapter-2',
      name: 'Adapter 2',
      state: 'hot',
      isLoading: false,
      error: undefined,
    }],
  ]),
  isCheckingAdapters: false,
  allAdaptersReady: true,
  unreadyAdapters: [],
  sseConnected: true,
  loadAllAdapters: vi.fn().mockResolvedValue(undefined),
  checkAdapterReadiness: vi.fn().mockReturnValue(true),
  showAdapterPrompt: false,
  dismissAdapterPrompt: vi.fn(),
  continueWithUnready: vi.fn(),
};

const mockSSEResult = {
  connected: false,
};

vi.mock('@/hooks/useModelStatus', () => ({
  useModelStatus: vi.fn(() => mockModelStatus),
}));

vi.mock('@/hooks/chat/useChatAdapterState', () => ({
  useChatAdapterState: vi.fn(() => mockAdapterState),
}));

vi.mock('@/hooks/useSSE', () => ({
  useSSE: vi.fn(() => mockSSEResult),
}));

describe('useModelLoadingState', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset mocks to default state
    mockModelStatus.status = 'loaded';
    mockModelStatus.isReady = true;
    mockModelStatus.errorMessage = null;
    mockAdapterState.allAdaptersReady = true;
    mockAdapterState.isCheckingAdapters = false;
    // Reset to default adapter states (both ready)
    mockAdapterState.adapterStates = new Map([
      ['adapter-1', {
        adapterId: 'adapter-1',
        name: 'Adapter 1',
        state: 'warm',
        isLoading: false,
        error: undefined,
      }],
      ['adapter-2', {
        adapterId: 'adapter-2',
        name: 'Adapter 2',
        state: 'hot',
        isLoading: false,
        error: undefined,
      }],
    ]);
  });

  describe('readiness states', () => {
    it('returns isReady=true when model loaded and all adapters ready', () => {
      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.isReady).toBe(true);
      expect(result.current.overallReady).toBe(true);
      expect(result.current.baseModelReady).toBe(true);
      expect(result.current.allAdaptersReady).toBe(true);
    });

    it('returns isReady=false when model not loaded', () => {
      mockModelStatus.status = 'loading';
      mockModelStatus.isReady = false;

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.isReady).toBe(false);
      expect(result.current.baseModelReady).toBe(false);
    });

    it('returns isReady=false when adapters not ready', () => {
      mockAdapterState.allAdaptersReady = false;

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.isReady).toBe(false);
      expect(result.current.allAdaptersReady).toBe(false);
    });
  });

  describe('loading states', () => {
    it('returns isLoading=true when model is loading', () => {
      mockModelStatus.status = 'loading';
      mockModelStatus.isReady = false;

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.isLoading).toBe(true);
    });

    it('returns isLoading=true when adapters are being checked', () => {
      mockAdapterState.isCheckingAdapters = true;

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.isLoading).toBe(true);
    });

    it('returns isLoading=false when everything is ready', () => {
      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.isLoading).toBe(false);
    });
  });

  describe('progress calculation', () => {
    it('returns progress=100 when everything is ready', () => {
      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.progress).toBe(100);
    });

    it('returns progress=0 when model not started and no adapters ready', () => {
      mockModelStatus.status = 'no-model';
      mockModelStatus.isReady = false;
      mockAdapterState.allAdaptersReady = false;
      mockAdapterState.adapterStates = new Map([
        ['adapter-1', {
          adapterId: 'adapter-1',
          name: 'Adapter 1',
          state: 'cold',
          isLoading: false,
          error: undefined,
        }],
      ]);

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      // Model weight is 0.3, adapter weight is 0.7
      // Model: 0%, Adapters: 0/1 ready = 0%
      // Progress = 0.3 * 0 + 0.7 * 0 = 0
      expect(result.current.progress).toBe(0);
    });

    it('calculates progress with mixed ready/loading adapters', () => {
      mockAdapterState.adapterStates = new Map([
        ['adapter-1', {
          adapterId: 'adapter-1',
          name: 'Adapter 1',
          state: 'warm',
          isLoading: false,
          error: undefined,
        }],
        ['adapter-2', {
          adapterId: 'adapter-2',
          name: 'Adapter 2',
          state: 'cold',
          isLoading: true,
          error: undefined,
        }],
      ]);
      mockAdapterState.allAdaptersReady = false;

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      // Model: 100% (ready), Adapters: 1/2 ready = 50%
      // Progress = 0.3 * 100 + 0.7 * 50 = 30 + 35 = 65
      expect(result.current.progress).toBe(65);
    });
  });

  describe('ETA calculation', () => {
    it('returns null estimatedTimeRemaining when not loading', () => {
      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.estimatedTimeRemaining).toBeNull();
    });

    it('calculates ETA based on loading adapters', () => {
      mockAdapterState.isCheckingAdapters = true;
      mockAdapterState.allAdaptersReady = false;
      mockAdapterState.adapterStates = new Map([
        ['adapter-1', {
          adapterId: 'adapter-1',
          name: 'Adapter 1',
          state: 'cold',
          isLoading: true,
          error: undefined,
        }],
        ['adapter-2', {
          adapterId: 'adapter-2',
          name: 'Adapter 2',
          state: 'cold',
          isLoading: true,
          error: undefined,
        }],
      ]);

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      // 2 adapters loading @ 8s each = 16s
      expect(result.current.estimatedTimeRemaining).toBe(16);
    });
  });

  describe('adapter arrays', () => {
    it('returns arrays for loadingAdapters, readyAdapters, failedAdapters', () => {
      mockAdapterState.adapterStates = new Map([
        ['adapter-1', {
          adapterId: 'adapter-1',
          name: 'Ready Adapter',
          state: 'warm',
          isLoading: false,
          error: undefined,
        }],
        ['adapter-2', {
          adapterId: 'adapter-2',
          name: 'Loading Adapter',
          state: 'cold',
          isLoading: true,
          error: undefined,
        }],
        ['adapter-3', {
          adapterId: 'adapter-3',
          name: 'Failed Adapter',
          state: 'unloaded',
          isLoading: false,
          error: 'Load failed',
        }],
      ]);

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(Array.isArray(result.current.loadingAdapters)).toBe(true);
      expect(Array.isArray(result.current.readyAdapters)).toBe(true);
      expect(Array.isArray(result.current.failedAdapters)).toBe(true);

      expect(result.current.loadingAdapters).toHaveLength(1);
      expect(result.current.loadingAdapters[0].name).toBe('Loading Adapter');

      expect(result.current.readyAdapters).toHaveLength(1);
      expect(result.current.readyAdapters[0].name).toBe('Ready Adapter');

      expect(result.current.failedAdapters).toHaveLength(1);
      expect(result.current.failedAdapters[0].name).toBe('Failed Adapter');
    });
  });

  describe('baseModel object', () => {
    it('returns grouped baseModel object per PRD spec', () => {
      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.baseModel).toBeDefined();
      expect(result.current.baseModel.status).toBe('loaded');
      expect(result.current.baseModel.modelName).toBe('Test Model');
      expect(result.current.baseModel.modelId).toBe('test-model-id');
      expect(result.current.baseModel.memoryUsageMb).toBe(4096);
      expect(result.current.baseModel.errorMessage).toBeNull();
    });
  });

  describe('SSE connection', () => {
    it('returns sseConnected status', () => {
      mockAdapterState.sseConnected = true;

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.sseConnected).toBe(true);
    });
  });

  describe('refresh actions', () => {
    it('provides refresh function that calls refreshAll', async () => {
      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(typeof result.current.refresh).toBe('function');

      await act(async () => {
        await result.current.refresh();
      });

      expect(mockModelStatus.refresh).toHaveBeenCalled();
    });
  });

  describe('error handling', () => {
    it('returns error when model has error', () => {
      mockModelStatus.status = 'error';
      mockModelStatus.errorMessage = 'Model load failed';

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.error).not.toBeNull();
      expect(result.current.error?.code).toBe('BASE_MODEL_LOAD_FAILED');
    });

    it('returns error when adapter has error', () => {
      mockModelStatus.status = 'loaded';
      mockAdapterState.adapterStates = new Map([
        ['adapter-1', {
          adapterId: 'adapter-1',
          name: 'Failed Adapter',
          state: 'unloaded',
          isLoading: false,
          error: 'Connection failed',
        }],
      ]);

      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      expect(result.current.error).not.toBeNull();
      expect(result.current.error?.code).toBe('ADAPTER_LOAD_FAILED');
    });
  });

  describe('backwards compatibility', () => {
    it('provides deprecated properties for backwards compatibility', () => {
      const { result } = renderHook(() => useModelLoadingState({ stackId: 'test-stack' }));

      // Deprecated aliases should still work
      expect(result.current.overallReady).toBe(result.current.isReady);
      expect(result.current.isConnected).toBe(result.current.sseConnected);
      expect(result.current.refreshAll).toBeDefined();
      expect(result.current.refreshBaseModel).toBeDefined();
      expect(result.current.refreshAdapters).toBeDefined();
    });
  });
});
