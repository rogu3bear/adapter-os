/**
 * Unit Tests for useAdapterStates Hook
 *
 * Tests the lightweight adapter state monitoring via SSE.
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useAdapterStates } from '@/hooks/model-loading/useAdapterStates';

// Mock SSE hook
const mockSSEResult = {
  connected: true,
  reconnect: vi.fn(),
};

let sseOnMessage: ((event: unknown) => void) | undefined;

vi.mock('@/hooks/useSSE', () => ({
  useSSE: vi.fn((url: string, options: { enabled?: boolean; onMessage?: (event: unknown) => void }) => {
    sseOnMessage = options.onMessage;
    return mockSSEResult;
  }),
}));

describe('useAdapterStates', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sseOnMessage = undefined;
    mockSSEResult.connected = true;
  });

  describe('initial state', () => {
    it('starts with empty adapters map', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      expect(result.current.adapters).toBeInstanceOf(Map);
      expect(result.current.adapters.size).toBe(0);
    });

    it('reflects SSE connection status', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      expect(result.current.connected).toBe(true);
    });

    it('starts with allReady=true when no adapters', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      expect(result.current.allReady).toBe(true);
    });

    it('starts with anyLoading=false', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      expect(result.current.anyLoading).toBe(false);
    });
  });

  describe('SSE subscription', () => {
    it('subscribes to adapter SSE stream', async () => {
      renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      // SSE hook should have been called
      const sseModule = await import('@/hooks/useSSE');
      expect(sseModule.useSSE).toHaveBeenCalled();
    });

    it('can be disabled via enabled option', async () => {
      renderHook(() => useAdapterStates({ stackId: 'test-stack', enabled: false }));

      const sseModule = await import('@/hooks/useSSE');
      const call = (sseModule.useSSE as unknown as ReturnType<typeof vi.fn>).mock.calls[0];
      expect(call[1].enabled).toBe(false);
    });
  });

  describe('state updates from SSE events', () => {
    it('updates state on SSE state transition events', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      // Simulate SSE event with correct shape
      act(() => {
        if (sseOnMessage) {
          sseOnMessage({
            adapter_id: 'adapter-1',
            adapter_name: 'Adapter 1',
            current_state: 'warm',
            previous_state: 'cold',
            reason: 'Loaded by user',
            activation_percentage: 85,
            timestamp: Date.now(),
          });
        }
      });

      expect(result.current.adapters.size).toBe(1);
      expect(result.current.adapters.get('adapter-1')).toBeDefined();
      expect(result.current.adapters.get('adapter-1')?.state).toBe('warm');
    });

    it('updates allReady after state changes', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      // Add a ready adapter
      act(() => {
        if (sseOnMessage) {
          sseOnMessage({
            adapter_id: 'adapter-1',
            adapter_name: 'Adapter 1',
            current_state: 'warm',
            previous_state: 'cold',
            activation_percentage: 100,
            timestamp: Date.now(),
          });
        }
      });

      expect(result.current.allReady).toBe(true);

      // Add a loading adapter
      act(() => {
        if (sseOnMessage) {
          sseOnMessage({
            adapter_id: 'adapter-2',
            adapter_name: 'Adapter 2',
            current_state: 'cold',
            previous_state: 'unloaded',
            activation_percentage: 0,
            timestamp: Date.now(),
          });
        }
      });

      expect(result.current.allReady).toBe(false);
    });
  });

  describe('getAdapter', () => {
    it('returns adapter state by ID', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      // Add an adapter
      act(() => {
        if (sseOnMessage) {
          sseOnMessage({
            adapter_id: 'adapter-1',
            adapter_name: 'Test Adapter',
            current_state: 'hot',
            previous_state: 'warm',
            activation_percentage: 95,
            timestamp: Date.now(),
          });
        }
      });

      const state = result.current.getAdapter('adapter-1');
      expect(state).toBeDefined();
      expect(state?.state).toBe('hot');
      expect(state?.name).toBe('Test Adapter');
    });

    it('returns undefined for unknown adapter', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      const state = result.current.getAdapter('unknown-adapter');
      expect(state).toBeUndefined();
    });
  });

  describe('anyLoading', () => {
    it('returns true when adapter is in cold state', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      act(() => {
        if (sseOnMessage) {
          sseOnMessage({
            adapter_id: 'adapter-1',
            adapter_name: 'Loading Adapter',
            current_state: 'cold',
            previous_state: 'unloaded',
            activation_percentage: 0,
            timestamp: Date.now(),
          });
        }
      });

      expect(result.current.anyLoading).toBe(true);
    });

    it('returns false when all adapters are warm/hot/resident', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      act(() => {
        if (sseOnMessage) {
          sseOnMessage({
            adapter_id: 'adapter-1',
            adapter_name: 'Ready Adapter',
            current_state: 'warm',
            previous_state: 'cold',
            activation_percentage: 100,
            timestamp: Date.now(),
          });
        }
      });

      expect(result.current.anyLoading).toBe(false);
    });
  });

  describe('reconnect', () => {
    it('provides reconnect function', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      expect(typeof result.current.reconnect).toBe('function');
    });

    it('calls SSE reconnect when invoked', () => {
      const { result } = renderHook(() => useAdapterStates({ stackId: 'test-stack' }));

      act(() => {
        result.current.reconnect();
      });

      expect(mockSSEResult.reconnect).toHaveBeenCalled();
    });
  });

  describe('callback handling', () => {
    it('calls onStateChange callback when adapter state changes', () => {
      const onStateChange = vi.fn();

      renderHook(() =>
        useAdapterStates({
          stackId: 'test-stack',
          onStateChange,
        })
      );

      act(() => {
        if (sseOnMessage) {
          sseOnMessage({
            adapter_id: 'adapter-1',
            adapter_name: 'Adapter 1',
            current_state: 'warm',
            previous_state: 'cold',
            activation_percentage: 100,
            timestamp: Date.now(),
          });
        }
      });

      expect(onStateChange).toHaveBeenCalledWith('adapter-1', 'warm');
    });
  });
});
