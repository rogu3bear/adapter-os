import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { TraceTimeline } from '@/components/observability/TraceTimeline';
import { apiClient } from '@/api/services';

// Mock apiClient
vi.mock('@/api/client', () => ({
  default: {
    searchTraces: vi.fn(),
    getTrace: vi.fn(),
  },
}));

// Mock usePolling hook
const mockRefetch = vi.fn();
vi.mock('@/hooks/usePolling', () => ({
  usePolling: vi.fn(() => ({
    data: [],
    isLoading: false,
    refetch: mockRefetch,
  })),
}));

describe('TraceTimeline', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders trace search interface', () => {
    (apiClient.searchTraces as any).mockResolvedValue([]);
    
    render(<TraceTimeline />);
    
    expect(screen.getByText('Trace Search')).toBeInTheDocument();
    expect(screen.getByText('Trace Details')).toBeInTheDocument();
  });

  it('displays empty state when no traces found', async () => {
    (apiClient.searchTraces as any).mockResolvedValue([]);
    
    render(<TraceTimeline />);
    
    await waitFor(() => {
      expect(screen.getByText('No traces found')).toBeInTheDocument();
    });
  });

  it('displays loading state', async () => {
    // Dynamically mock usePolling for this test
    const usePollingModule = await import('@/hooks/usePolling');
    vi.mocked(usePollingModule.usePolling).mockReturnValue({
      data: [],
      isLoading: true,
      refetch: mockRefetch,
    });

    render(<TraceTimeline />);

    // Should show loading indicator when searching
    expect(screen.getByText('Searching traces...')).toBeInTheDocument();
  });

  it('displays trace list when traces are available', async () => {
    const traceIds = ['trace-1', 'trace-2', 'trace-3'];

    // Dynamically mock usePolling
    const usePollingModule = await import('@/hooks/usePolling');
    vi.mocked(usePollingModule.usePolling).mockReturnValue({
      data: traceIds,
      isLoading: false,
      refetch: mockRefetch,
    });

    render(<TraceTimeline />);

    await waitFor(() => {
      traceIds.forEach(id => {
        expect(screen.getByText(id)).toBeInTheDocument();
      });
    });
  });

  it('displays trace details when trace is selected', async () => {
    const traceIds = ['trace-1'];
    const trace = {
      trace_id: 'trace-1',
      spans: [
        {
          span_id: 'span-1',
          trace_id: 'trace-1',
          parent_id: null,
          name: 'test-span',
          start_ns: 1000000,
          end_ns: 2000000,
          attributes: {},
          status: 'ok' as const,
        },
      ],
      root_span_id: 'span-1',
    };

    // Dynamically mock usePolling
    const usePollingModule = await import('@/hooks/usePolling');
    vi.mocked(usePollingModule.usePolling).mockReturnValue({
      data: traceIds,
      isLoading: false,
      refetch: mockRefetch,
    });

    (apiClient.getTrace as any).mockResolvedValue(trace);

    render(<TraceTimeline />);

    await waitFor(() => {
      expect(screen.getByText('trace-1')).toBeInTheDocument();
    });

    // Click on trace
    const traceButton = screen.getByText('trace-1').closest('button');
    if (traceButton) {
      traceButton.click();

      await waitFor(() => {
        expect(screen.getByText('test-span')).toBeInTheDocument();
      });
    }
  });

  it('handles empty trace selection gracefully', () => {
    (apiClient.searchTraces as any).mockResolvedValue([]);
    
    render(<TraceTimeline />);
    
    expect(screen.getByText('Select a trace to view its span timeline and details')).toBeInTheDocument();
  });
});

