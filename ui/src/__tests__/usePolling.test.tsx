import React from 'react';
import { render, screen, waitFor, act } from '@testing-library/react';
import { vi } from 'vitest';
import { usePolling } from '../hooks/usePolling';

// Mock React to track renders
let renderCount = 0;
const mockSetState = vi.fn();
const originalUseState = React.useState;

// Create a test component that uses usePolling
function TestPollingComponent({ fetchFn, config }: { fetchFn: () => Promise<any>, config?: any }) {
  renderCount++;
  const { data, isLoading, error } = usePolling(fetchFn, 'fast', config);

  return (
    <div>
      <div data-testid="render-count">{renderCount}</div>
      <div data-testid="data">{JSON.stringify(data)}</div>
      <div data-testid="loading">{isLoading ? 'loading' : 'not-loading'}</div>
      <div data-testid="error">{error ? error.message : 'no-error'}</div>
    </div>
  );
}

describe('usePolling Effect Dependency Churn', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    renderCount = 0;
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('should minimize re-renders when config changes frequently', async () => {
    const mockFetch = vi.fn().mockResolvedValue({ value: 42 });

    // Initial render
    const { rerender } = render(
      <TestPollingComponent
        fetchFn={mockFetch}
        config={{ enabled: true, intervalMs: 5000 }}
      />
    );

    // Wait for initial fetch
    await waitFor(() => {
      expect(screen.getByTestId('loading')).toHaveTextContent('not-loading');
    });

    await waitFor(() => {
      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    const initialRenderCount = renderCount;

    // Change config frequently (this should not cause excessive re-renders)
    rerender(
      <TestPollingComponent
        fetchFn={mockFetch}
        config={{ enabled: true, intervalMs: 3000 }}
      />
    );

    rerender(
      <TestPollingComponent
        fetchFn={mockFetch}
        config={{ enabled: true, intervalMs: 7000 }}
      />
    );

    rerender(
      <TestPollingComponent
        fetchFn={mockFetch}
        config={{ enabled: true, intervalMs: 2000 }}
      />
    );

    // Advance time to trigger polling
    act(() => {
      vi.advanceTimersByTime(3000);
    });

    // Flush promises
    await act(async () => {
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(mockFetch).toHaveBeenCalledTimes(2); // Initial + 1 poll
    });

    // Should not have excessive re-renders (allow some for state updates)
    const finalRenderCount = renderCount;
    const rendersAfterConfigChanges = finalRenderCount - initialRenderCount;

    // With proper memoization, config changes should not cause many re-renders
    expect(rendersAfterConfigChanges).toBeLessThan(10);
  });

  it('should handle fetchFn changes without breaking polling', async () => {
    const mockFetch1 = vi.fn().mockResolvedValue({ value: 1 });
    const mockFetch2 = vi.fn().mockResolvedValue({ value: 2 });

    const { rerender } = render(
      <TestPollingComponent fetchFn={mockFetch1} />
    );

    // Initial fetch
    await waitFor(() => {
      expect(mockFetch1).toHaveBeenCalledTimes(1);
    });

    // Change fetch function
    rerender(<TestPollingComponent fetchFn={mockFetch2} />);

    // Advance time to trigger next poll with new function
    act(() => {
      vi.advanceTimersByTime(3000);
    });

    await act(async () => {
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(mockFetch2).toHaveBeenCalledTimes(1);
    });

    // Should still be polling correctly
    act(() => {
      vi.advanceTimersByTime(3000);
    });

    await act(async () => {
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(mockFetch2).toHaveBeenCalledTimes(2);
    });
  });

  it('should not create overlapping intervals when config changes', async () => {
    const mockFetch = vi.fn().mockResolvedValue({ value: 42 });

    const { rerender } = render(
      <TestPollingComponent
        fetchFn={mockFetch}
        config={{ enabled: true, intervalMs: 5000 }}
      />
    );

    // Initial fetch
    await waitFor(() => {
      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    // Change interval rapidly multiple times
    rerender(
      <TestPollingComponent
        fetchFn={mockFetch}
        config={{ enabled: true, intervalMs: 1000 }}
      />
    );

    rerender(
      <TestPollingComponent
        fetchFn={mockFetch}
        config={{ enabled: true, intervalMs: 2000 }}
      />
    );

    rerender(
      <TestPollingComponent
        fetchFn={mockFetch}
        config={{ enabled: true, intervalMs: 500 }}
      />
    );

    // Advance time - should only trigger one fetch per interval, not multiple overlapping ones
    act(() => {
      vi.advanceTimersByTime(1000);
    });

    // Flush promises and timers
    await act(async () => {
      await Promise.resolve();
      vi.runOnlyPendingTimers();
    });

    // Give time for any overlapping intervals to fire
    await new Promise(resolve => setTimeout(resolve, 10));

    // Should have exactly 2 fetches: initial + 1 poll (not more due to overlapping intervals)
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });
});
