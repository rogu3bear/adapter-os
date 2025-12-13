import { describe, expect, it, vi, beforeEach } from 'vitest';
import userEvent from '@testing-library/user-event';
import { screen } from '@testing-library/react';
import DashboardPage from '@/pages/DashboardPage';
import { renderWithProviders } from './utils/testProviders';
import { useSystemMetrics, useMetricsSnapshot } from '@/hooks/useSystem';

vi.mock('@/hooks/useSystem', () => ({
  useSystemMetrics: vi.fn(),
  useMetricsSnapshot: vi.fn(),
}));

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({
    selectedTenant: 'test-tenant',
    setSelectedTenant: vi.fn(),
    tenants: [],
    isLoading: false,
    refreshTenants: vi.fn(),
  }),
}));

vi.mock('@/hooks/useTraining', () => ({
  useTraining: {
    useDatasets: () => ({ data: null, isLoading: false, error: null }),
    useTrainingJobs: () => ({ data: null, isLoading: false, error: null }),
  },
}));

vi.mock('@/hooks/useReposApi', () => ({
  useRepos: () => ({ data: null, isLoading: false, error: null }),
}));

vi.mock('@/components/ModelSelector', () => ({
  ModelSelector: () => <div data-testid="model-selector" />,
}));

vi.mock('@/components/dashboard/index', () => ({
  __esModule: true,
  default: () => <div data-testid="role-dashboard" />,
}));

vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: { role: 'admin', display_name: 'Tester', email: 'tester@example.com' },
  }),
}));

vi.mock('@/utils/logger', () => ({
  logger: {
    error: vi.fn(),
    warn: vi.fn(),
    info: vi.fn(),
    debug: vi.fn(),
  },
  toError: (e: unknown) => (e instanceof Error ? e : new Error(String(e))),
}));

const useSystemMetricsMock = vi.mocked(useSystemMetrics);
const useMetricsSnapshotMock = vi.mocked(useMetricsSnapshot);

const baseSystemReturn = {
  metrics: null,
  isLoading: false,
  error: null,
  lastUpdated: null,
  refetch: vi.fn(),
};

const baseSnapshotReturn = {
  data: null,
  isLoading: false,
  error: null,
  lastUpdated: null,
  refetch: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
  useSystemMetricsMock.mockReturnValue(baseSystemReturn);
  useMetricsSnapshotMock.mockReturnValue(baseSnapshotReturn);
});

describe('DashboardPage metric cards', () => {
  it('shows loading state for health and traffic cards', () => {
    useSystemMetricsMock.mockReturnValue({ ...baseSystemReturn, isLoading: true });
    useMetricsSnapshotMock.mockReturnValue({ ...baseSnapshotReturn, isLoading: true });

    renderWithProviders(<DashboardPage />);

    expect(screen.getByText(/loading system health/i)).toBeInTheDocument();
    expect(screen.getByText(/loading traffic metrics/i)).toBeInTheDocument();
  });

  it('shows error state with retry when hooks error', async () => {
    const refetchMetrics = vi.fn();
    const refetchSnapshot = vi.fn();
    useSystemMetricsMock.mockReturnValue({
      ...baseSystemReturn,
      error: new Error('boom'),
      refetch: refetchMetrics,
    });
    useMetricsSnapshotMock.mockReturnValue({
      ...baseSnapshotReturn,
      error: new Error('snap'),
      refetch: refetchSnapshot,
    });

    renderWithProviders(<DashboardPage />);

    expect(screen.getAllByText(/unable to load metrics/i).length).toBeGreaterThanOrEqual(1);

    const retryButtons = screen.getAllByRole('button', { name: /retry/i });
    await userEvent.click(retryButtons[0]);

    expect(refetchMetrics).toHaveBeenCalled();
    expect(refetchSnapshot).toHaveBeenCalled();
  });

  it('shows empty state when data is missing', () => {
    renderWithProviders(<DashboardPage />);

    const emptyStates = screen.getAllByText(/no recent data/i);
    expect(emptyStates.length).toBeGreaterThanOrEqual(1);
  });

  it('renders metric values when data is available', () => {
    useSystemMetricsMock.mockReturnValue({
      ...baseSystemReturn,
      metrics: {
        cpu_usage_percent: 12.34,
        memory_usage_percent: 45.67,
        tokens_per_second: 9.8,
        error_rate: 0.0123,
        active_sessions: 3,
      },
    });
    useMetricsSnapshotMock.mockReturnValue({
      ...baseSnapshotReturn,
      data: {
        schema_version: '1',
        timestamp: new Date().toISOString(),
        metrics: {},
        gauges: {
          adapteros_requests_per_min: 42.2,
        },
        counters: {},
        labels: {},
        histograms: {},
      },
    });

    renderWithProviders(<DashboardPage />);

    expect(screen.getByText(/CPU: 12.3%/i)).toBeInTheDocument();
    expect(screen.getByText(/Memory: 45.7%/i)).toBeInTheDocument();
    expect(screen.getAllByText(/Tokens\/sec: 9.8/i).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/Error rate: 1\.23%/i).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/Requests\/min: 42.2/i)).toBeInTheDocument();
  });
});

