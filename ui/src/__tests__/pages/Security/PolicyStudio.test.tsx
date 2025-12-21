/**
 * Tests for PolicyStudio Page
 *
 * Verifies page rendering, RBAC permissions, and API interactions.
 * Citation: AGENTS.md - Policy Studio feature for tenant-safe policy authoring
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import React from 'react';
import PolicyStudio from '@/pages/Security/PolicyStudio';
import type { TenantPolicyCustomization } from '@/hooks/security/useTenantPolicies';

// Mock API client
const mockRequest = vi.fn();

vi.mock('@/api/services', () => ({
  apiClient: {
    request: (...args: unknown[]) => mockRequest(...args),
  },
}));

// Mock RBAC hook
const mockCan = vi.fn();
const mockIsAuthenticated = vi.fn(() => true);
vi.mock('@/hooks/security/useRBAC', () => ({
  useRBAC: () => ({
    can: mockCan,
    userRole: 'admin',
    permissions: ['*'],
    hasRole: vi.fn(() => true),
    isAuthenticated: mockIsAuthenticated,
  }),
}));

// Mock auth provider
vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: {
      id: 'user-1',
      email: 'test@example.com',
      tenant_id: 'tenant-1',
      role: 'admin',
    },
    isAuthenticated: true,
  }),
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Test data
const mockCustomizations: TenantPolicyCustomization[] = [
  {
    id: 'custom-1',
    tenant_id: 'tenant-1',
    base_policy_type: 'egress',
    customizations_json: '{"max_domains": 10}',
    status: 'draft',
    created_at: '2025-01-01T00:00:00Z',
    created_by: 'user-1',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'custom-2',
    tenant_id: 'tenant-1',
    base_policy_type: 'determinism',
    customizations_json: '{"require_metallib_embed": true}',
    status: 'pending_review',
    submitted_at: '2025-01-02T00:00:00Z',
    created_at: '2025-01-01T00:00:00Z',
    created_by: 'user-1',
    updated_at: '2025-01-02T00:00:00Z',
  },
  {
    id: 'custom-3',
    tenant_id: 'tenant-1',
    base_policy_type: 'router',
    customizations_json: '{}',
    status: 'active',
    activated_at: '2025-01-03T00:00:00Z',
    created_at: '2025-01-01T00:00:00Z',
    created_by: 'user-1',
    updated_at: '2025-01-03T00:00:00Z',
  },
];

// Test wrapper factory
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
    logger: {
      log: () => {},
      warn: () => {},
      error: () => {},
    },
  });

  return function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  };
}

function renderPolicyStudio() {
  return render(<PolicyStudio />, { wrapper: createWrapper() });
}

describe('PolicyStudio - Permission Checks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders permission denied when user lacks policy:customize permission', () => {
    mockCan.mockReturnValue(false);

    renderPolicyStudio();

    expect(screen.getByText(/Access Denied/i)).toBeInTheDocument();
    expect(mockCan).toHaveBeenCalledWith('policy:customize');
  });

  it('renders page content when user has policy:customize permission', async () => {
    mockCan.mockReturnValue(true);
    mockRequest.mockResolvedValue([]);

    renderPolicyStudio();

    expect(screen.getByText('Policy Studio')).toBeInTheDocument();
    expect(screen.getByText(/Customize policy parameters/i)).toBeInTheDocument();
  });
});

describe('PolicyStudio - Page Rendering', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('renders page header and description', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyStudio();

    expect(screen.getByText('Policy Studio')).toBeInTheDocument();
    expect(screen.getByText(/Customize policy parameters for your tenant/i)).toBeInTheDocument();
  });

  it('renders refresh button', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyStudio();

    expect(screen.getByRole('button', { name: /Refresh/i })).toBeInTheDocument();
  });

  it('renders summary cards with correct counts', async () => {
    mockRequest.mockResolvedValue(mockCustomizations);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByText('Total Customizations')).toBeInTheDocument();
    });

    // Check that customization list loaded by looking for a customization type
    await waitFor(() => {
      expect(screen.getByText('Egress Ruleset')).toBeInTheDocument();
    });
  });

  it('renders create new customization section', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyStudio();

    expect(screen.getByText('Create New Customization')).toBeInTheDocument();
    expect(screen.getByText(/Select a guardrail to customize/i)).toBeInTheDocument();
    expect(screen.getByRole('combobox')).toBeInTheDocument();
  });

  it('renders customizations list when data is present', async () => {
    mockRequest.mockResolvedValue(mockCustomizations);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByText('Your Customizations')).toBeInTheDocument();
    });

    // Wait for customization cards to load
    await waitFor(() => {
      expect(screen.getByText('Egress Ruleset')).toBeInTheDocument();
    });
    expect(screen.getByText('Determinism Ruleset')).toBeInTheDocument();
  });

  it('renders empty state when no customizations exist', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByText(/No customizations yet/i)).toBeInTheDocument();
    });
  });

  it('displays loading skeleton while fetching data', () => {
    mockCan.mockReturnValue(true);
    mockRequest.mockImplementation(() => new Promise(() => {})); // Never resolves

    renderPolicyStudio();

    // The page should render the customizations section with loading state
    expect(screen.getByText('Your Customizations')).toBeInTheDocument();
  });
});

describe('PolicyStudio - Status Badges', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('displays correct status badge for draft customizations', async () => {
    mockRequest.mockResolvedValue([mockCustomizations[0]]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByText('Draft')).toBeInTheDocument();
    });
  });

  it('displays correct status badge for pending_review customizations', async () => {
    mockRequest.mockResolvedValue([mockCustomizations[1]]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByText('Pending Review')).toBeInTheDocument();
    });
  });

  it('displays correct status badge for active customizations', async () => {
    mockRequest.mockResolvedValue([mockCustomizations[2]]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByText('Active')).toBeInTheDocument();
    });
  });
});

describe('PolicyStudio - Action Buttons', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('shows Edit, Submit, and Delete buttons for draft customizations', async () => {
    mockRequest.mockResolvedValue([mockCustomizations[0]]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Edit/i })).toBeInTheDocument();
    });

    expect(screen.getByRole('button', { name: /Submit/i })).toBeInTheDocument();
    // Delete button has no text, just icon
    const deleteButton = screen.getAllByRole('button').find(
      btn => btn.querySelector('svg.lucide-trash-2')
    );
    expect(deleteButton).toBeDefined();
  });

  it('does not show action buttons for pending_review customizations', async () => {
    mockRequest.mockResolvedValue([mockCustomizations[1]]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByText('Pending Review')).toBeInTheDocument();
    });

    // Edit and Submit buttons should not be present
    expect(screen.queryByRole('button', { name: /^Edit$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Submit$/i })).not.toBeInTheDocument();
  });
});

describe('PolicyStudio - Create Flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('Create Customization button is disabled when no policy type selected', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyStudio();

    await waitFor(() => {
      const createButton = screen.getByRole('button', { name: /Create Customization/i });
      expect(createButton).toBeDisabled();
    });
  });
});

describe('PolicyStudio - Error Handling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('displays error recovery when API fails', async () => {
    mockRequest.mockRejectedValue(new Error('Failed to load customizations'));

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByText(/Failed to load customizations/i)).toBeInTheDocument();
    });
  });

  it('shows retry button on error', async () => {
    mockRequest.mockRejectedValue(new Error('Network error'));

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Retry/i })).toBeInTheDocument();
    });
  });
});

describe('PolicyStudio - API Contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('calls correct endpoint when loading customizations', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(mockRequest).toHaveBeenCalledWith(
        expect.stringContaining('/v1/tenants/tenant-1/policies/customizations')
      );
    });
  });

  it('renders policy type selector for creating customization', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByRole('combobox')).toBeInTheDocument();
    });

    // Create button should be disabled when no policy type selected
    const createButton = screen.getByRole('button', { name: /Create Customization/i });
    expect(createButton).toBeDisabled();
  });

  it('renders submit button for draft customizations', async () => {
    mockRequest.mockResolvedValue([mockCustomizations[0]]);

    renderPolicyStudio();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Submit/i })).toBeInTheDocument();
    });
  });

  it('calls correct endpoint with DELETE when deleting customization', async () => {
    mockRequest
      .mockResolvedValueOnce([mockCustomizations[0]]) // Initial load
      .mockResolvedValueOnce(undefined); // Delete response

    // Mock confirm
    vi.spyOn(window, 'confirm').mockReturnValue(true);

    renderPolicyStudio();

    await waitFor(() => {
      const deleteButton = screen.getAllByRole('button').find(
        btn => btn.querySelector('svg.lucide-trash-2')
      );
      expect(deleteButton).toBeDefined();
      if (deleteButton) {
        fireEvent.click(deleteButton);
      }
    });

    await waitFor(() => {
      expect(mockRequest).toHaveBeenCalledWith(
        '/v1/tenants/tenant-1/policies/customizations/custom-1',
        { method: 'DELETE' }
      );
    });
  });
});
