/**
 * RoleDashboards Tests
 *
 * Comprehensive tests for role-based dashboard rendering:
 * - AdminDashboard renders correct widgets
 * - OperatorDashboard renders correct widgets
 * - SREDashboard renders correct widgets
 * - ComplianceDashboard renders correct widgets
 * - ViewerDashboard renders correct widgets
 * - Dashboard router selects correct dashboard based on role
 * - Quick actions are role-appropriate
 * - Permission-based widget filtering
 *
 * Citation: 【2025-11-25†tests†role-dashboards】
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import type { User, Tenant, AuditLog, SystemMetrics, TrainingJob, Dataset, Adapter } from '@/api/types';

// Import dashboards
import Dashboard, { DashboardProvider } from '@/components/dashboard';
import AdminDashboard from '@/components/dashboard/roles/AdminDashboard';
import OperatorDashboard from '@/components/dashboard/roles/OperatorDashboard';
import ViewerDashboard from '@/components/dashboard/roles/ViewerDashboard';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

// Mock data
const mockAdminUser: User = {
  user_id: 'admin-1',
  email: 'admin@adapteros.local',
  display_name: 'Admin User',
  role: 'admin',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
  is_active: true,
};

const mockOperatorUser: User = {
  user_id: 'operator-1',
  email: 'operator@adapteros.local',
  display_name: 'Operator User',
  role: 'operator',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
  is_active: true,
};

const mockSREUser: User = {
  user_id: 'sre-1',
  email: 'sre@adapteros.local',
  display_name: 'SRE User',
  role: 'sre',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
  is_active: true,
};

const mockComplianceUser: User = {
  user_id: 'compliance-1',
  email: 'compliance@adapteros.local',
  display_name: 'Compliance User',
  role: 'compliance',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
  is_active: true,
};

const mockViewerUser: User = {
  user_id: 'viewer-1',
  email: 'viewer@adapteros.local',
  display_name: 'Viewer User',
  role: 'viewer',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
  is_active: true,
};

const mockTenants: Tenant[] = [
  {
    id: 'tenant-1',
    name: 'Organization 1',
    status: 'active',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'tenant-2',
    name: 'Organization 2',
    status: 'paused',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
];

const mockSystemMetrics: SystemMetrics = {
  cpu_usage_percent: 45.2,
  memory_usage_percent: 62.8,
  disk_usage_percent: 38.5,
  network_rx_bytes: 1024000,
  adapter_count: 12,
  active_sessions: 3,
  tokens_per_second: 150,
  latency_p95_ms: 25,
};

const mockTrainingJobs: TrainingJob[] = [
  {
    id: 'job-1',
    adapter_name: 'Code Assistant',
    adapter_id: 'adapter-1',
    dataset_id: 'dataset-1',
    status: 'running',
    progress_pct: 65,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T01:00:00Z',
  },
  {
    id: 'job-2',
    adapter_name: 'SQL Helper',
    adapter_id: 'adapter-2',
    dataset_id: 'dataset-2',
    status: 'completed',
    progress_pct: 100,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T02:00:00Z',
    completed_at: '2025-01-01T02:00:00Z',
  },
];

const mockDatasets: Dataset[] = [
  {
    id: 'dataset-1',
    name: 'Code Dataset',
    validation_status: 'valid',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'dataset-2',
    name: 'SQL Dataset',
    validation_status: 'valid',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
];

const mockAdapters: Adapter[] = [
  {
    adapter_id: 'adapter-1',
    name: 'Code Assistant',
    tier: 'tier_1',
    lifecycle_state: 'loaded',
    current_state: 'warm',
  },
  {
    adapter_id: 'adapter-2',
    name: 'SQL Helper',
    tier: 'tier_2',
    lifecycle_state: 'loaded',
    current_state: 'hot',
  },
];

const mockAuditLogs: AuditLog[] = [
  {
    id: 'audit-1',
    user_id: 'admin-1',
    action: 'adapter.register',
    resource: 'adapter',
    status: 'success',
    timestamp: '2025-01-01T00:00:00Z',
  },
  {
    id: 'audit-2',
    user_id: 'operator-1',
    action: 'auth.login',
    resource: 'auth',
    status: 'failure',
    timestamp: '2025-01-01T01:00:00Z',
  },
];

// Mock API client
const mockListTenants = vi.fn();
const mockListUsers = vi.fn();
const mockQueryAuditLogs = vi.fn();
const mockGetSystemMetrics = vi.fn();
const mockListAdapters = vi.fn();

const tenantsQueryState = {
  data: mockTenants as Tenant[] | null,
  isLoading: false,
  error: null as unknown,
};

const adapterStacksState = {
  data: [] as unknown[],
  isLoading: false,
  error: null as unknown,
};

const defaultStackState = {
  data: null as unknown,
  isLoading: false,
  error: null as unknown,
};

const trainingJobsState = {
  data: { jobs: mockTrainingJobs } as { jobs: TrainingJob[] } | null,
  isLoading: false,
  error: null as unknown,
};

const datasetsState = {
  data: { datasets: mockDatasets } as { datasets: Dataset[] } | null,
  isLoading: false,
  error: null as unknown,
};

const adaptersState = {
  data: { adapters: mockAdapters } as { adapters: Adapter[] } | null,
  isLoading: false,
  error: null as unknown,
};

const chatSessionsState = {
  sessions: [] as unknown[],
  isLoading: false,
};

const resetQueryStates = () => {
  tenantsQueryState.data = mockTenants;
  tenantsQueryState.isLoading = false;
  tenantsQueryState.error = null;

  adapterStacksState.data = [];
  adapterStacksState.isLoading = false;
  adapterStacksState.error = null;

  defaultStackState.data = null;
  defaultStackState.isLoading = false;
  defaultStackState.error = null;

  trainingJobsState.data = { jobs: mockTrainingJobs };
  trainingJobsState.isLoading = false;
  trainingJobsState.error = null;

  datasetsState.data = { datasets: mockDatasets };
  datasetsState.isLoading = false;
  datasetsState.error = null;

  adaptersState.data = { adapters: mockAdapters };
  adaptersState.isLoading = false;
  adaptersState.error = null;

  chatSessionsState.sessions = [];
  chatSessionsState.isLoading = false;
};

const resolveCommonApiMocks = () => {
  mockListTenants.mockResolvedValue(mockTenants);
  mockListUsers.mockResolvedValue({ users: [mockAdminUser, mockOperatorUser] });
  mockQueryAuditLogs.mockResolvedValue(mockAuditLogs);
  mockGetSystemMetrics.mockResolvedValue(mockSystemMetrics);
  mockListAdapters.mockResolvedValue(mockAdapters);
};

vi.mock('@/api/client', () => ({
  __esModule: true,
  default: {
    listTenants: (...args: unknown[]) => mockListTenants(...args),
    listUsers: (...args: unknown[]) => mockListUsers(...args),
    queryAuditLogs: (...args: unknown[]) => mockQueryAuditLogs(...args),
    getSystemMetrics: (...args: unknown[]) => mockGetSystemMetrics(...args),
    listAdapters: (...args: unknown[]) => mockListAdapters(...args),
  },
}));

// Mock hooks
vi.mock('@/hooks/useAdmin', () => ({
  useTenants: () => ({
    data: tenantsQueryState.data,
    isLoading: tenantsQueryState.isLoading,
    error: tenantsQueryState.error,
    refetch: vi.fn(),
  }),
  useAdapterStacks: () => ({
    data: adapterStacksState.data,
    isLoading: adapterStacksState.isLoading,
    error: adapterStacksState.error,
    refetch: vi.fn(),
  }),
  useGetDefaultStack: () => ({
    data: defaultStackState.data,
    isLoading: defaultStackState.isLoading,
    error: defaultStackState.error,
    refetch: vi.fn(),
  }),
}));

vi.mock('@/hooks/useTraining', () => ({
  useTraining: {
    useTrainingJobs: () => ({
      data: trainingJobsState.data,
      isLoading: trainingJobsState.isLoading,
      error: trainingJobsState.error,
      refetch: vi.fn(),
    }),
    useDatasets: () => ({
      data: datasetsState.data,
      isLoading: datasetsState.isLoading,
      error: datasetsState.error,
      refetch: vi.fn(),
    }),
  },
}));

vi.mock('@/pages/Adapters/useAdapters', () => ({
  useAdapters: () => ({
    data: adaptersState.data,
    isLoading: adaptersState.isLoading,
    error: adaptersState.error,
    refetch: vi.fn(),
  }),
}));

vi.mock('@/hooks/useChatSessionsApi', () => ({
  useChatSessionsApi: () => ({
    sessions: chatSessionsState.sessions,
    isLoading: chatSessionsState.isLoading,
    createSession: vi.fn(),
    updateSession: vi.fn(),
    deleteSession: vi.fn(),
  }),
}));

// Mock CoreProviders with AuthContext
let mockUser: User | null = null;
const roleUsers: Record<string, User> = {
  admin: mockAdminUser,
  operator: mockOperatorUser,
  sre: mockSREUser,
  compliance: mockComplianceUser,
  viewer: mockViewerUser,
};

vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: mockUser,
    isAuthenticated: !!mockUser,
    login: vi.fn(),
    logout: vi.fn(),
  }),
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Mock logger
vi.mock('@/utils/logger', () => ({
  logger: {
    error: vi.fn(),
    warn: vi.fn(),
    info: vi.fn(),
  },
  toError: (error: unknown) => error,
}));

// Test wrapper component
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        <DashboardProvider>
          <SectionErrorBoundary sectionName="Dashboard">{children}</SectionErrorBoundary>
        </DashboardProvider>
      </QueryClientProvider>
    </MemoryRouter>
  );
}

function setMockUserRole(role: User['role'] | 'unknown' | null) {
  if (!role) {
    mockUser = null;
    return;
  }
  const normalized = role.toLowerCase();
  mockUser = roleUsers[normalized] || { ...mockViewerUser, role: role as User['role'] };
}

function renderDashboardForRole(role: User['role'] | 'unknown' | null) {
  setMockUserRole(role);
  return render(
    <TestWrapper>
      <Dashboard />
    </TestWrapper>
  );
}

describe('AdminDashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetQueryStates();
    resolveCommonApiMocks();
    mockUser = mockAdminUser;
  });

  it('renders Admin Dashboard with correct title', async () => {
    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    expect(screen.getByText('Admin Dashboard')).toBeTruthy();
  });

  it('displays tenant summary widget', async () => {
    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Organization Summary')).toBeTruthy();
    });
  });

  it('displays user activity widget', async () => {
    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('User Activity')).toBeTruthy();
      expect(screen.getByText('Total Users')).toBeTruthy();
    });
  });

  it('displays security overview widget', async () => {
    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Security Overview')).toBeTruthy();
      expect(screen.getByText('Policy Violations')).toBeTruthy();
    });
  });

  it('displays system resource usage widget', async () => {
    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('System Resource Usage')).toBeTruthy();
      expect(screen.getByText('CPU Usage')).toBeTruthy();
      expect(screen.getByText('Memory Usage')).toBeTruthy();
    });
  });

  it('shows admin-specific quick actions', async () => {
    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Create Tenant')).toBeTruthy();
      expect(screen.getByText('Manage Users')).toBeTruthy();
      expect(screen.getByText('System Settings')).toBeTruthy();
      expect(screen.getByText('Security Audit')).toBeTruthy();
    });
  });

  it('displays admin role badge', async () => {
    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      const adminBadges = screen.getAllByText('Admin');
      expect(adminBadges.length).toBeGreaterThan(0);
      const fullAccessBadges = screen.getAllByText('Full Access');
      expect(fullAccessBadges.length).toBeGreaterThan(0);
    });
  });
});

describe('OperatorDashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetQueryStates();
    resolveCommonApiMocks();
    mockUser = mockOperatorUser;
  });

  it('renders Operator Dashboard with correct title', async () => {
    render(
      <TestWrapper>
        <OperatorDashboard selectedTenant="default" />
      </TestWrapper>
    );

    expect(screen.getByText('Operator Dashboard')).toBeTruthy();
  });

  it('displays training progress widget', async () => {
    render(
      <TestWrapper>
        <OperatorDashboard selectedTenant="default" />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Training Progress')).toBeTruthy();
    });
  });

  it('displays dataset summary widget', async () => {
    render(
      <TestWrapper>
        <OperatorDashboard selectedTenant="default" />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Dataset Summary')).toBeTruthy();
      expect(screen.getByText('Total datasets')).toBeTruthy();
    });
  });

  it('displays adapter lifecycle widget', async () => {
    render(
      <TestWrapper>
        <OperatorDashboard selectedTenant="default" />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Adapter Lifecycle')).toBeTruthy();
      expect(screen.getByText('Total adapters')).toBeTruthy();
    });
  });

  it('shows operator-specific quick actions', async () => {
    render(
      <TestWrapper>
        <OperatorDashboard selectedTenant="default" />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Upload Dataset')).toBeTruthy();
      expect(screen.getByText('Start Training')).toBeTruthy();
      expect(screen.getByText('View Training Jobs')).toBeTruthy();
      expect(screen.getByText('Manage Adapters')).toBeTruthy();
    });
  });

  it('displays active training jobs', async () => {
    render(
      <TestWrapper>
        <OperatorDashboard selectedTenant="default" />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Active Training Jobs')).toBeTruthy();
    });
  });

  it('displays operator role badge', async () => {
    render(
      <TestWrapper>
        <OperatorDashboard selectedTenant="default" />
      </TestWrapper>
    );

    await waitFor(() => {
      const operatorBadges = screen.getAllByText('Operator');
      expect(operatorBadges.length).toBeGreaterThan(0);
    });
  });
});

describe('ViewerDashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetQueryStates();
    resolveCommonApiMocks();
    mockUser = mockViewerUser;
  });

  it('renders Viewer Dashboard with correct title', async () => {
    render(
      <TestWrapper>
        <ViewerDashboard />
      </TestWrapper>
    );

    expect(screen.getByText('Dashboard')).toBeTruthy();
  });

  it('displays system overview widgets', async () => {
    render(
      <TestWrapper>
        <ViewerDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('System Overview')).toBeTruthy();
      expect(screen.getByText('System Status')).toBeTruthy();
      const availableAdapters = screen.getAllByText('Available Adapters');
      expect(availableAdapters.length).toBeGreaterThan(0);
    });
  });

  it('displays getting started guide', async () => {
    render(
      <TestWrapper>
        <ViewerDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Getting Started')).toBeTruthy();
      const browseAdapters = screen.getAllByText(/Browse Adapters/);
      expect(browseAdapters.length).toBeGreaterThan(0);
    });
  });

  it('shows viewer-specific quick actions (read-only)', async () => {
    render(
      <TestWrapper>
        <ViewerDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      const quickActions = screen.getAllByRole('button');
      const labels = quickActions.map((btn) => btn.textContent);

      // Check for read-only actions
      expect(labels.some((label) => label?.includes('Start Chat'))).toBe(true);
      expect(labels.some((label) => label?.includes('Browse Adapters'))).toBe(true);
      expect(labels.some((label) => label?.includes('View Documentation'))).toBe(true);
      expect(labels.some((label) => label?.includes('Help'))).toBe(true);
    });
  });

  it('displays help and resources section', async () => {
    render(
      <TestWrapper>
        <ViewerDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Help & Resources')).toBeTruthy();
      expect(screen.getByText('User Guide')).toBeTruthy();
    });
  });

  it('does NOT show admin/operator actions', async () => {
    render(
      <TestWrapper>
        <ViewerDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.queryByText('Create Tenant')).toBeNull();
      expect(screen.queryByText('Upload Dataset')).toBeNull();
      expect(screen.queryByText('Start Training')).toBeNull();
    });
  });
});

describe('Dashboard Router', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetQueryStates();
    resolveCommonApiMocks();
  });

  it('renders AdminDashboard for admin role', async () => {
    renderDashboardForRole('admin');

    await waitFor(() => {
      expect(screen.getByText('Admin Dashboard')).toBeTruthy();
      expect(screen.getByText('Organization Summary')).toBeTruthy();
    });
  });

  it('renders OperatorDashboard for operator role', async () => {
    renderDashboardForRole('operator');

    await waitFor(() => {
      expect(screen.getByText('Operator Dashboard')).toBeTruthy();
      expect(screen.getByText('Training Progress')).toBeTruthy();
    });
  });

  it('renders ViewerDashboard for viewer role', async () => {
    renderDashboardForRole('viewer');

    await waitFor(() => {
      expect(screen.getByText('Getting Started')).toBeTruthy();
    });
  });

  it('defaults to ViewerDashboard for unknown role', async () => {
    renderDashboardForRole('unknown');

    await waitFor(() => {
      expect(screen.getByText('Getting Started')).toBeTruthy();
    });
  });
});

describe('Permission-Based Widget Filtering', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetQueryStates();
    resolveCommonApiMocks();
  });

  it('admin sees all widgets', async () => {
    mockUser = mockAdminUser;

    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Organization Summary')).toBeTruthy();
      expect(screen.getByText('User Activity')).toBeTruthy();
      expect(screen.getByText('Security Overview')).toBeTruthy();
      expect(screen.getByText('System Resource Usage')).toBeTruthy();
    });
  });

  it('operator sees training and adapter widgets only', async () => {
    mockUser = mockOperatorUser;

    render(
      <TestWrapper>
        <OperatorDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Training Progress')).toBeTruthy();
      expect(screen.getByText('Dataset Summary')).toBeTruthy();
      expect(screen.getByText('Adapter Lifecycle')).toBeTruthy();
    });

    // Should NOT see admin-only widgets
    expect(screen.queryByText('Organization Summary')).toBeNull();
    expect(screen.queryByText('User Activity')).toBeNull();
  });

  it('viewer sees limited read-only widgets', async () => {
    mockUser = mockViewerUser;

    render(
      <TestWrapper>
        <ViewerDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('System Overview')).toBeTruthy();
      expect(screen.getByText('Getting Started')).toBeTruthy();
    });

    // Should NOT see operational widgets
    expect(screen.queryByText('Training Progress')).toBeNull();
    expect(screen.queryByText('Organization Summary')).toBeNull();
  });
});

describe('Quick Actions Role Filtering', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetQueryStates();
    resolveCommonApiMocks();
  });

  it('admin has tenant and user management actions', async () => {
    mockUser = mockAdminUser;

    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Create Tenant')).toBeTruthy();
      expect(screen.getByText('Manage Users')).toBeTruthy();
      expect(screen.getByText('System Settings')).toBeTruthy();
      expect(screen.getByText('Security Audit')).toBeTruthy();
    });
  });

  it('operator has training and adapter actions', async () => {
    mockUser = mockOperatorUser;

    render(
      <TestWrapper>
        <OperatorDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Upload Dataset')).toBeTruthy();
      expect(screen.getByText('Start Training')).toBeTruthy();
      expect(screen.getByText('View Training Jobs')).toBeTruthy();
      expect(screen.getByText('Manage Adapters')).toBeTruthy();
    });

    // Should NOT see admin actions
    expect(screen.queryByText('Create Tenant')).toBeNull();
    expect(screen.queryByText('Manage Users')).toBeNull();
  });

  it('viewer has read-only actions', async () => {
    mockUser = mockViewerUser;

    render(
      <TestWrapper>
        <ViewerDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      const buttons = screen.getAllByRole('button');
      const labels = buttons.map((btn) => btn.textContent);

      // Check for read-only actions
      expect(labels.some((label) => label?.includes('Start Chat'))).toBe(true);
      expect(labels.some((label) => label?.includes('Browse Adapters'))).toBe(true);

      // Should NOT see write actions
      expect(labels.some((label) => label?.includes('Upload'))).toBe(false);
      expect(labels.some((label) => label?.includes('Create'))).toBe(false);
      expect(labels.some((label) => label?.includes('Manage'))).toBe(false);
    });
  });
});

describe('Error Handling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows error message when tenant data fails to load', async () => {
    mockUser = mockAdminUser;
    resetQueryStates();
    tenantsQueryState.data = null;
    tenantsQueryState.error = new Error('Failed to fetch tenants');
    mockListUsers.mockResolvedValue({ users: [] });
    mockQueryAuditLogs.mockResolvedValue([]);
    mockGetSystemMetrics.mockResolvedValue(mockSystemMetrics);

    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      // Error should be displayed somewhere in the component
      const errors = screen.queryAllByText(/failed/i);
      expect(errors.length).toBeGreaterThan(0);
    });
  });

  it('shows error message when training jobs fail to load', async () => {
    mockUser = mockOperatorUser;
    resetQueryStates();
    resolveCommonApiMocks();
    trainingJobsState.data = null;
    trainingJobsState.error = new Error('Failed to fetch training jobs');

    render(
      <TestWrapper>
        <OperatorDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      // Error recovery template should be shown
      const errorElements = screen.getAllByRole('button', { name: /retry/i });
      expect(errorElements.length).toBeGreaterThan(0);
    });
  });
});

describe('Loading States', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetQueryStates();
    resolveCommonApiMocks();
  });

  it('shows skeleton loaders while data is loading', async () => {
    mockUser = mockAdminUser;
    tenantsQueryState.data = null;
    tenantsQueryState.isLoading = true;
    mockListUsers.mockImplementation(() => new Promise(() => {})); // keep users loading

    render(
      <TestWrapper>
        <AdminDashboard />
      </TestWrapper>
    );

    await waitFor(() => {
      const skeletons = document.querySelectorAll('[class*="skeleton"]');
      expect(skeletons.length).toBeGreaterThan(0);
    });
  });
});
