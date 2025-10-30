import { render, screen, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import { RealtimeMetrics } from '../components/RealtimeMetrics';
import * as api from '../api/client';

// Mock api client default export and named apiClient
vi.mock('../api/client', () => {
  const mock = {
    getSystemMetrics: vi.fn(),
    subscribeToMetrics: vi.fn(() => () => {}),
  };
  return {
    __esModule: true,
    default: mock,
    apiClient: mock,
  };
});

const mockUser = { id: '1', name: 'Test User' };
const mockTenant = 'default';

describe('RealtimeMetrics', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (api as any).apiClient.getSystemMetrics.mockResolvedValue(null);
  });

  it('renders without crashing', () => {
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);
    expect(screen.getByText('Real-time Metrics')).toBeInTheDocument();
  });

  it('displays default metrics on null data', async () => {
    (api.apiClient.getSystemMetrics as any).mockResolvedValue(null);
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getAllByText('0%').length).toBeGreaterThan(0);
    });
  });

  it('updates metrics on successful fetch', async () => {
    const mockData = { cpu_usage_percent: 25, memory_usage_pct: 40, gpu_utilization_percent: 60, latency_p95_ms: 15 };
    (api.apiClient.getSystemMetrics as any).mockResolvedValue(mockData);
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getByText(/25(\.0)?%/)).toBeInTheDocument(); // CPU
      expect(screen.getByText(/40(\.0)?%/)).toBeInTheDocument(); // Memory
    });
  });

  it('handles fetch error gracefully', async () => {
    (api.apiClient.getSystemMetrics as any).mockRejectedValue(new Error('Network error'));
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getAllByText('0%').length).toBeGreaterThan(0);
    });
  });

  it('connects to SSE and parses events', async () => {
    (api.apiClient.subscribeToMetrics as any).mockImplementation((cb: any) => {
      cb({ cpu_usage_percent: 30, memory_usage_pct: 10, gpu_utilization_percent: 0, tokens_per_second: 0, latency_p95_ms: 0 });
      return () => {};
    });

    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getByText(/30(\.0)?%/)).toBeInTheDocument();
    });
  });

  it('falls back to polling when EventSource is unavailable', async () => {
    // Force fallback path by removing EventSource
    vi.stubGlobal('EventSource', undefined as any);
    const intervalSpy = vi.spyOn(global, 'setInterval');
    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(intervalSpy).toHaveBeenCalledWith(expect.any(Function), 500);
    });
  });

  it('handles SSE disconnect notifications', async () => {
    (api.apiClient.subscribeToMetrics as any).mockImplementation((cb: any) => {
      cb(null);
      return () => {};
    });

    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getByText('Real-time Metrics')).toBeInTheDocument();
    });
  });

  it.skip('handles multiple SSE events', async () => {
    (api.apiClient.subscribeToMetrics as any).mockImplementation((cb: any) => {
      cb({ cpu_usage_percent: 35, memory_usage_pct: 0, gpu_utilization_percent: 0, tokens_per_second: 0, latency_p95_ms: 0 });
      return () => {};
    });

    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      const matches = screen.queryAllByText(/35(\.0)?%/);
      expect(matches.length).toBeGreaterThan(0);
    });
  });
});
