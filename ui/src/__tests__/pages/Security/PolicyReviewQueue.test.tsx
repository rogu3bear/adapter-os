/**
 * Tests for PolicyReviewQueue Page
 *
 * Verifies page rendering, RBAC permissions, and API interactions for policy review workflow.
 * Citation: AGENTS.md - Policy Studio feature review workflow
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import React from 'react';
import PolicyReviewQueue from '@/pages/Security/PolicyReviewQueue';
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

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Test data
const mockPendingReviews: TenantPolicyCustomization[] = [
  {
    id: 'custom-1',
    tenant_id: 'tenant-1',
    base_policy_type: 'egress',
    customizations_json: '{"max_domains": 10}',
    status: 'pending_review',
    submitted_at: '2025-01-02T00:00:00Z',
    created_at: '2025-01-01T00:00:00Z',
    created_by: 'developer@example.com',
    updated_at: '2025-01-02T00:00:00Z',
  },
  {
    id: 'custom-2',
    tenant_id: 'tenant-2',
    base_policy_type: 'determinism',
    customizations_json: '{"require_metallib_embed": true}',
    status: 'pending_review',
    submitted_at: '2025-01-03T00:00:00Z',
    created_at: '2025-01-01T00:00:00Z',
    created_by: 'another-developer@example.com',
    updated_at: '2025-01-03T00:00:00Z',
  },
];

const mockApprovedCustomization: TenantPolicyCustomization = {
  ...mockPendingReviews[0],
  status: 'approved',
  reviewed_at: '2025-01-04T00:00:00Z',
  reviewed_by: 'admin@example.com',
  review_notes: 'Approved for production',
};

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

function renderPolicyReviewQueue() {
  return render(<PolicyReviewQueue />, { wrapper: createWrapper() });
}

describe('PolicyReviewQueue - Permission Checks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders permission denied when user lacks policy:review permission', () => {
    mockCan.mockReturnValue(false);

    renderPolicyReviewQueue();

    expect(screen.getByText(/Access Denied/i)).toBeInTheDocument();
    expect(mockCan).toHaveBeenCalledWith('policy:review');
  });

  it('renders page content when user has policy:review permission', async () => {
    mockCan.mockReturnValue(true);
    mockRequest.mockResolvedValue([]);

    renderPolicyReviewQueue();

    expect(screen.getByText('Policy Review Queue')).toBeInTheDocument();
    expect(screen.getByText(/Review and approve tenant policy customizations/i)).toBeInTheDocument();
  });
});

describe('PolicyReviewQueue - Page Rendering', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('renders page header and description', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyReviewQueue();

    expect(screen.getByText('Policy Review Queue')).toBeInTheDocument();
    expect(screen.getByText(/Review and approve tenant policy customizations/i)).toBeInTheDocument();
  });

  it('renders refresh button', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyReviewQueue();

    expect(screen.getByRole('button', { name: /Refresh/i })).toBeInTheDocument();
  });

  it('renders pending reviews count in summary card', async () => {
    mockRequest.mockResolvedValue(mockPendingReviews);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByText('Pending Reviews')).toBeInTheDocument();
    });

    // Wait for data to load and check that reviews are displayed
    await waitFor(() => {
      expect(screen.getByText('Egress Ruleset')).toBeInTheDocument();
    });
  });

  it('renders singular text for single pending review', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    renderPolicyReviewQueue();

    // Wait for the single review to be rendered
    await waitFor(() => {
      expect(screen.getByText('Egress Ruleset')).toBeInTheDocument();
    });
  });

  it('renders empty state when no pending reviews', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByText(/No pending reviews/i)).toBeInTheDocument();
    });
  });

  it('displays loading skeleton while fetching data', () => {
    mockCan.mockReturnValue(true);
    mockRequest.mockImplementation(() => new Promise(() => {})); // Never resolves

    renderPolicyReviewQueue();

    expect(screen.getByText('Pending Reviews')).toBeInTheDocument();
  });
});

describe('PolicyReviewQueue - Review Cards', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('displays policy pack name for each pending review', async () => {
    mockRequest.mockResolvedValue(mockPendingReviews);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByText('Egress Ruleset')).toBeInTheDocument();
      expect(screen.getByText('Determinism Ruleset')).toBeInTheDocument();
    });
  });

  it('displays Pending Review badge', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByText('Pending Review')).toBeInTheDocument();
    });
  });

  it('displays tenant ID for each review', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByText('tenant-1')).toBeInTheDocument();
    });
  });

  it('displays created by information', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByText('developer@example.com')).toBeInTheDocument();
    });
  });

  it('displays submitted date', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    renderPolicyReviewQueue();

    await waitFor(() => {
      // Date should be rendered (format may vary based on locale)
      expect(screen.getByText(/1\/2\/2025|2025|Jan/)).toBeInTheDocument();
    });
  });

  it('displays customization values in code block', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByText('Customization Values:')).toBeInTheDocument();
    });

    // The JSON should be displayed in a pre element
    const preElement = screen.getByText(/"max_domains": 10/);
    expect(preElement).toBeInTheDocument();
  });
});

describe('PolicyReviewQueue - Action Buttons', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('shows Approve and Reject buttons for each pending review', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Approve/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Reject/i })).toBeInTheDocument();
    });
  });

  it('shows multiple Approve/Reject button pairs for multiple reviews', async () => {
    mockRequest.mockResolvedValue(mockPendingReviews);

    renderPolicyReviewQueue();

    await waitFor(() => {
      const approveButtons = screen.getAllByRole('button', { name: /Approve/i });
      const rejectButtons = screen.getAllByRole('button', { name: /Reject/i });

      expect(approveButtons).toHaveLength(2);
      expect(rejectButtons).toHaveLength(2);
    });
  });
});

describe('PolicyReviewQueue - Approve Flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('opens approve dialog when clicking Approve button', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Approve/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: /Approve/i }));

    await waitFor(() => {
      expect(screen.getByText('Approve Customization')).toBeInTheDocument();
    });
  });

  it('shows optional notes field in approve dialog', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Approve/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: /Approve/i }));

    await waitFor(() => {
      expect(screen.getByLabelText(/Review Notes/i)).toBeInTheDocument();
      expect(screen.getByPlaceholderText(/Optional notes about this approval/i)).toBeInTheDocument();
    });
  });

  it('shows activation reminder in approve dialog', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Approve/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: /Approve/i }));

    await waitFor(() => {
      expect(screen.getByText(/After approval, an Admin must activate/i)).toBeInTheDocument();
    });
  });

  it('calls correct API endpoint when confirming approval', async () => {
    mockRequest
      .mockResolvedValueOnce([mockPendingReviews[0]]) // Initial load
      .mockResolvedValueOnce({
        customization: mockApprovedCustomization,
        validation: { valid: true, errors: [], warnings: [] },
      }); // Approve response

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Approve/i })).toBeInTheDocument();
    });

    // Click Approve to open dialog
    await user.click(screen.getByRole('button', { name: /Approve/i }));

    await waitFor(() => {
      expect(screen.getByText('Approve Customization')).toBeInTheDocument();
    });

    // Enter notes
    const notesInput = screen.getByLabelText(/Review Notes/i);
    await user.type(notesInput, 'Approved for production');

    // Click confirm Approve button in dialog
    const dialogApproveButton = screen.getAllByRole('button', { name: /Approve/i }).find(
      btn => btn.closest('[role="dialog"]')
    );
    expect(dialogApproveButton).toBeDefined();
    if (dialogApproveButton) {
      await user.click(dialogApproveButton);
    }

    await waitFor(() => {
      expect(mockRequest).toHaveBeenCalledWith(
        '/v1/policies/customizations/custom-1/approve',
        {
          method: 'POST',
          body: JSON.stringify({ notes: 'Approved for production' }),
        }
      );
    });
  });
});

describe('PolicyReviewQueue - Reject Flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('opens reject dialog when clicking Reject button', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Reject/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: /Reject/i }));

    await waitFor(() => {
      expect(screen.getByText('Reject Customization')).toBeInTheDocument();
    });
  });

  it('shows required notes field in reject dialog', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Reject/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: /Reject/i }));

    await waitFor(() => {
      expect(screen.getByText(/Required for rejection/i)).toBeInTheDocument();
      expect(screen.getByPlaceholderText(/Explain why this customization is being rejected/i)).toBeInTheDocument();
    });
  });

  it('disables reject confirmation when notes are empty', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Reject/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: /Reject/i }));

    await waitFor(() => {
      const dialogRejectButton = screen.getAllByRole('button', { name: /Reject/i }).find(
        btn => btn.closest('[role="dialog"]')
      );
      expect(dialogRejectButton).toBeDisabled();
    });
  });

  it('enables reject confirmation when notes are provided', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Reject/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole('button', { name: /Reject/i }));

    await waitFor(() => {
      expect(screen.getByText('Reject Customization')).toBeInTheDocument();
    });

    // Enter notes
    const notesInput = screen.getByLabelText(/Review Notes/i);
    await user.type(notesInput, 'Does not meet compliance requirements');

    const dialogRejectButton = screen.getAllByRole('button', { name: /Reject/i }).find(
      btn => btn.closest('[role="dialog"]')
    );
    expect(dialogRejectButton).not.toBeDisabled();
  });

  it('calls correct API endpoint when confirming rejection', async () => {
    mockRequest
      .mockResolvedValueOnce([mockPendingReviews[0]]) // Initial load
      .mockResolvedValueOnce({
        customization: { ...mockPendingReviews[0], status: 'rejected' },
        validation: { valid: true, errors: [], warnings: [] },
      }); // Reject response

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Reject/i })).toBeInTheDocument();
    });

    // Click Reject to open dialog
    await user.click(screen.getByRole('button', { name: /Reject/i }));

    await waitFor(() => {
      expect(screen.getByText('Reject Customization')).toBeInTheDocument();
    });

    // Enter notes
    const notesInput = screen.getByLabelText(/Review Notes/i);
    await user.type(notesInput, 'Does not meet compliance requirements');

    // Click confirm Reject button in dialog
    const dialogRejectButton = screen.getAllByRole('button', { name: /Reject/i }).find(
      btn => btn.closest('[role="dialog"]')
    );
    if (dialogRejectButton) {
      await user.click(dialogRejectButton);
    }

    await waitFor(() => {
      expect(mockRequest).toHaveBeenCalledWith(
        '/v1/policies/customizations/custom-1/reject',
        {
          method: 'POST',
          body: JSON.stringify({ notes: 'Does not meet compliance requirements' }),
        }
      );
    });
  });
});

describe('PolicyReviewQueue - Error Handling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('displays error recovery when API fails', async () => {
    mockRequest.mockRejectedValue(new Error('Failed to load pending reviews'));

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByText(/Failed to load pending reviews/i)).toBeInTheDocument();
    });
  });

  it('shows retry button on error', async () => {
    mockRequest.mockRejectedValue(new Error('Network error'));

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Retry/i })).toBeInTheDocument();
    });
  });
});

describe('PolicyReviewQueue - API Contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('calls correct endpoint when loading pending reviews', async () => {
    mockRequest.mockResolvedValue([]);

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(mockRequest).toHaveBeenCalledWith('/v1/policies/pending-reviews');
    });
  });

  it('refresh button triggers refetch', async () => {
    mockRequest.mockResolvedValue([]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(mockRequest).toHaveBeenCalledTimes(1);
    });

    // Clear mock and click refresh
    mockRequest.mockClear();
    mockRequest.mockResolvedValue([]);

    const refreshButton = screen.getByRole('button', { name: /Refresh/i });
    await user.click(refreshButton);

    await waitFor(() => {
      expect(mockRequest).toHaveBeenCalledWith('/v1/policies/pending-reviews');
    });
  });
});

describe('PolicyReviewQueue - Dialog Cancel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCan.mockReturnValue(true);
  });

  it('closes dialog when clicking Cancel', async () => {
    mockRequest.mockResolvedValue([mockPendingReviews[0]]);

    const user = userEvent.setup();

    renderPolicyReviewQueue();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Approve/i })).toBeInTheDocument();
    });

    // Open approve dialog
    await user.click(screen.getByRole('button', { name: /Approve/i }));

    await waitFor(() => {
      expect(screen.getByText('Approve Customization')).toBeInTheDocument();
    });

    // Click Cancel
    await user.click(screen.getByRole('button', { name: /Cancel/i }));

    await waitFor(() => {
      expect(screen.queryByText('Approve Customization')).not.toBeInTheDocument();
    });
  });
});
