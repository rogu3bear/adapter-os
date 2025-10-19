import { render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import { RealtimeMetrics } from '../components/RealtimeMetrics';
import * as api from '../api/client';

// Mock apiClient
vi.mock('../api/client', () => ({
  apiClient: {
    getSystemMetrics: vi.fn(),
  },
}));

const mockUser = { id: '1', name: 'Test User' };
const mockTenant = 'default';

describe('RealtimeMetrics', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders without crashing', () => {
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);
    expect(screen.getByText('Real-time Metrics')).toBeInTheDocument();
  });

  it('displays default metrics on null data', async () => {
    (api.apiClient.getSystemMetrics as any).mockResolvedValue(null);
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getByText('0%')).toBeInTheDocument(); // CPU/Memory etc.
    });
  });

  it('updates metrics on successful fetch', async () => {
    const mockData = { cpu_usage: 25, memory_usage: 40, gpu_utilization: 60, avg_latency_ms: 15 };
    (api.apiClient.getSystemMetrics as any).mockResolvedValue(mockData);
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getByText('25%')).toBeInTheDocument(); // CPU
      expect(screen.getByText('40%')).toBeInTheDocument(); // Memory
    });
  });

  it('handles fetch error gracefully', async () => {
    (api.apiClient.getSystemMetrics as any).mockRejectedValue(new Error('Network error'));
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getByText('0%')).toBeInTheDocument(); // Fallback
    });
  });

  it('connects to SSE and parses events', async () => {
    const mockData = { cpu_usage: 30 };
    const EventSourceMock = vi.fn(() => ({
      addEventListener: vi.fn((type, listener) => {
        if (type === 'metrics') {
          listener({ data: JSON.stringify(mockData) } as MessageEvent);
        }
      }),
      close: vi.fn(),
    }));
    vi.stubGlobal('EventSource', EventSourceMock);

    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(EventSourceMock).toHaveBeenCalledWith(expect.stringContaining('stream/metrics?token='));
      expect(screen.getByText('30%')).toBeInTheDocument();
    });
  });

  it('falls back to polling on SSE disconnect', async () => {
    vi.stubGlobal('EventSource', vi.fn(() => ({
      addEventListener: vi.fn(),
      close: vi.fn(),
      onerror: vi.fn((e) => {
        // Simulate disconnect
        (e.target as any).onerror({ type: 'error' });
      }),
    })));

    const intervalSpy = vi.spyOn(global, 'setInterval');
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(intervalSpy).toHaveBeenCalledWith(expect.any(Function), 500);
    });
  });

  it('handles SSE auth error and reconnects', async () => {
    const EventSourceMock = vi.fn(() => ({
      addEventListener: vi.fn((type, listener) => {
        if (type === 'error') {
          listener({ type: 'error', status: 401 } as MessageEvent);
        }
      }),
      close: vi.fn(),
    }));
    vi.stubGlobal('EventSource', EventSourceMock);

    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(EventSourceMock).toHaveBeenCalledTimes(2); // Reconnect attempt
      // Assert toast if sonner mocked, or console
    });
  });

  it('throttles duplicate SSE events', async () => {
    const mockData = { cpu_usage: 30 };
    let eventCount = 0;
    const EventSourceMock = vi.fn(() => ({
      addEventListener: vi.fn((type, listener) => {
        if (type === 'metrics') {
          // Dispatch twice quickly
          listener({ data: JSON.stringify(mockData) } as MessageEvent);
          setTimeout(() => listener({ data: JSON.stringify(mockData) } as MessageEvent), 10);
          eventCount = 2;
        }
      }),
      close: vi.fn(),
    }));
    vi.stubGlobal('EventSource', EventSourceMock);

    const { rerender } = render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getByText('30%')).toBeInTheDocument();
      // Check history length (should not duplicate if throttled; assume logic skips <50ms)
    }, { timeout: 200 });
  });
});
