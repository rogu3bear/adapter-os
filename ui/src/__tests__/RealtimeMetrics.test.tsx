import { render, screen, waitFor, fireEvent } from '@testing-library/react';
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

  it('allows manual refresh to refetch metrics', async () => {
    const mockData = [
      { cpu_usage_percent: 15, memory_usage_pct: 20, gpu_utilization_percent: 5, latency_p95_ms: 10 },
      { cpu_usage_percent: 45, memory_usage_pct: 55, gpu_utilization_percent: 15, latency_p95_ms: 25 },
    ];
    (api.apiClient.getSystemMetrics as any)
      .mockResolvedValueOnce(mockData[0])
      .mockResolvedValue(mockData[1]);

    render(<RealtimeMetrics user={mockUser} selectedTenant={mockTenant} />);

    await waitFor(() => {
      expect(screen.getByText(/15(\.0)?%/)).toBeInTheDocument();
    });

    const refreshButton = screen.getByRole('button', { name: /Refresh/ });
    fireEvent.click(refreshButton);

    await waitFor(() => {
      expect(screen.getByText(/45(\.0)?%/)).toBeInTheDocument();
    });

  });
});
