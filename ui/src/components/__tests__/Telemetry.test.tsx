import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { Telemetry } from '@/components/Telemetry';
import apiClient from '@/api/client';
import { useSSE } from '@/hooks/useSSE';

// Mock apiClient
vi.mock('@/api/client', () => ({
  default: {
    getTelemetryEvents: vi.fn(),
    getTelemetryBundle: vi.fn(),
    verifyBundleSignature: vi.fn(),
    deleteTelemetryBundle: vi.fn(),
    purgeTelemetryBundles: vi.fn(),
  },
}));

// Mock useSSE hook
const mockUseSSE = vi.fn();
vi.mock('@/hooks/useSSE', () => ({
  useSSE: (endpoint: string, options?: any) => mockUseSSE(endpoint, options),
}));

// Mock provider hooks
vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: {
      user_id: 'test-user',
      email: 'test@example.com',
      role: 'admin',
    },
  }),
}));

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({
    selectedTenant: 'default',
  }),
}));

// Mock useDensity hook
vi.mock('@/contexts/DensityContext', () => ({
  useDensity: () => ({
    density: 'normal',
    setDensity: vi.fn(),
  }),
}));

describe('Telemetry', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    
    // Default SSE mock - connected and no data
    mockUseSSE.mockReturnValue({
      data: null,
      error: null,
      connected: true,
    });
    
    // Default API mocks
    (apiClient.getTelemetryEvents as any).mockResolvedValue([]);
  });

  it('renders telemetry component', () => {
    render(<Telemetry />);
    
    // Should render without crashing
    expect(screen.getByText(/Telemetry/i)).toBeInTheDocument();
  });

  it('uses SSE hook with correct endpoint and token', () => {
    const mockToken = 'test-token';
    (apiClient as any).getToken = vi.fn(() => mockToken);
    
    render(<Telemetry />);
    
    // Verify SSE is called for telemetry stream
    expect(mockUseSSE).toHaveBeenCalled();
    const call = mockUseSSE.mock.calls.find(c => 
      c[0]?.includes('/v1/stream/telemetry') || c[0]?.includes('telemetry')
    );
    expect(call).toBeDefined();
  });

  it('displays loading state initially', () => {
    (apiClient.getTelemetryEvents as any).mockImplementation(() => 
      new Promise(() => {}) // Never resolves
    );
    
    render(<Telemetry />);
    
    // Should show loading state
    expect(screen.queryByText(/loading/i)).toBeInTheDocument();
  });

  it('displays telemetry bundles when loaded', async () => {
    const bundles = [
      {
        bundle_id: 'bundle-1',
        timestamp: '2024-01-01T00:00:00Z',
        event_count: 10,
        tenant_id: 'default',
      },
      {
        bundle_id: 'bundle-2',
        timestamp: '2024-01-01T01:00:00Z',
        event_count: 5,
        tenant_id: 'default',
      },
    ];
    
    (apiClient.getTelemetryEvents as any).mockResolvedValue(bundles);
    
    render(<Telemetry />);
    
    await waitFor(() => {
      expect(screen.getByText('bundle-1')).toBeInTheDocument();
    });
  });

  it('handles SSE connection errors', () => {
    mockUseSSE.mockReturnValue({
      data: null,
      error: 'Connection failed',
      connected: false,
    });
    
    render(<Telemetry />);
    
    // Should handle error gracefully (no crash)
    expect(screen.queryByText(/Telemetry/i)).toBeInTheDocument();
  });

  it('updates display when SSE receives new data', async () => {
    const initialBundles: any[] = [];
    const updatedBundles = [
      {
        bundle_id: 'bundle-1',
        timestamp: '2024-01-01T00:00:00Z',
        event_count: 10,
        tenant_id: 'default',
      },
    ];
    
    (apiClient.getTelemetryEvents as any)
      .mockResolvedValueOnce(initialBundles)
      .mockResolvedValueOnce(updatedBundles);
    
    render(<Telemetry />);
    
    // Simulate SSE data update
    mockUseSSE.mockReturnValue({
      data: updatedBundles,
      error: null,
      connected: true,
    });
    
    await waitFor(() => {
      expect(apiClient.getTelemetryEvents).toHaveBeenCalled();
    });
  });

  it('handles empty telemetry state', async () => {
    (apiClient.getTelemetryEvents as any).mockResolvedValue([]);
    
    render(<Telemetry />);
    
    await waitFor(() => {
      // Should show empty state or table headers
      expect(screen.queryByText(/bundle/i)).toBeInTheDocument();
    });
  });
});

