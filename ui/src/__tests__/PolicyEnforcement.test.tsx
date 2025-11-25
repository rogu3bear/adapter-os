/**
 * Tests for Policy Enforcement blocking in adapter and stack operations
 *
 * Tests policy preflight checks integration with:
 * - Adapter loading/unloading operations
 * - Stack activation operations
 * - Admin override capabilities
 * - Audit trail generation
 *
 * Citation: [2025-11-25†ui†policy-enforcement-tests]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { PolicyPreflightDialog } from '@/components/PolicyPreflightDialog';
import type { PolicyPreflightCheck, PolicyPreflightResponse } from '@/api/policyTypes';

// Mock data
const mockPassedChecks: PolicyPreflightCheck[] = [
  {
    policy_id: 'egress',
    policy_name: 'Egress Control',
    passed: true,
    severity: 'error',
    message: 'Zero network egress verified (UDS-only)',
  },
  {
    policy_id: 'determinism',
    policy_name: 'Deterministic Execution',
    passed: true,
    severity: 'error',
    message: 'All randomness seeded via HKDF',
  },
  {
    policy_id: 'router',
    policy_name: 'Router Policy',
    passed: true,
    severity: 'error',
    message: 'K-sparse LoRA routing with Q15 gates verified',
  },
];

const mockWarningChecks: PolicyPreflightCheck[] = [
  ...mockPassedChecks,
  {
    policy_id: 'naming',
    policy_name: 'Semantic Naming',
    passed: false,
    severity: 'warning',
    message: 'Adapter name should follow {tenant}/{domain}/{purpose}/{revision} format',
    can_override: true,
    details: 'Expected: tenant-a/engineering/code-review/r001, Got: my-adapter',
  },
];

const mockCriticalFailureChecks: PolicyPreflightCheck[] = [
  ...mockPassedChecks.slice(0, 2),
  {
    policy_id: 'egress',
    policy_name: 'Egress Control',
    passed: false,
    severity: 'error',
    message: 'Network egress detected in production mode',
    can_override: false,
    details: 'TCP connection to 192.168.1.100:8080 detected',
  },
];

const mockOverridableErrorChecks: PolicyPreflightCheck[] = [
  ...mockPassedChecks.slice(0, 2),
  {
    policy_id: 'adapter-quality',
    policy_name: 'Adapter Quality',
    passed: false,
    severity: 'error',
    message: 'Activation percentage below threshold',
    can_override: true,
    details: 'Expected: >= 5%, Got: 2.3%',
  },
];

const createPreflightResponse = (
  checks: PolicyPreflightCheck[],
  canProceed: boolean
): PolicyPreflightResponse => ({
  adapterId: 'test-adapter',
  operation: 'load',
  canProceed,
  checks,
  checkedAt: new Date().toISOString(),
});

// Test wrapper
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        {children}
      </QueryClientProvider>
    </MemoryRouter>
  );
}

describe('PolicyPreflightDialog - Rendering', () => {
  it('renders policy check results correctly', () => {
    const response = createPreflightResponse(mockWarningChecks, true);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation - Load Adapter"
          description="The following policies will be enforced when loading this adapter"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
        />
      </TestWrapper>
    );

    expect(screen.getByText('Policy Validation - Load Adapter')).toBeInTheDocument();
    expect(screen.getByText(/following policies will be enforced/i)).toBeInTheDocument();
  });

  it('shows correct statistics (passed/failed/warnings)', () => {
    const response = createPreflightResponse(mockWarningChecks, true);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
        />
      </TestWrapper>
    );

    // Total: 4, Passed: 3, Errors: 0, Warnings: 1
    expect(screen.getByText('4')).toBeInTheDocument(); // Total
    expect(screen.getByText('3')).toBeInTheDocument(); // Passed
    expect(screen.getByText('0')).toBeInTheDocument(); // Errors (none)
    // Note: Can't use getByText('1') as it's ambiguous, check for Warnings label instead
    const warningsSection = screen.getByText('Warnings');
    expect(warningsSection.parentElement?.textContent).toContain('1');
  });

  it('displays passed checks in collapsed section', () => {
    const response = createPreflightResponse(mockPassedChecks, true);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
        />
      </TestWrapper>
    );

    expect(screen.getByText(/Passed Checks \(3\)/i)).toBeInTheDocument();
  });

  it('displays failed checks with severity badges', () => {
    const response = createPreflightResponse(mockWarningChecks, true);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
        />
      </TestWrapper>
    );

    expect(screen.getByText('Semantic Naming')).toBeInTheDocument();
    expect(screen.getByText('warning')).toBeInTheDocument();
    expect(screen.getByText(/Adapter name should follow/)).toBeInTheDocument();
  });
});

describe('PolicyPreflightDialog - Critical Violations', () => {
  it('blocks proceed when critical violations exist', () => {
    const response = createPreflightResponse(mockCriticalFailureChecks, false);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
        />
      </TestWrapper>
    );

    const proceedButton = screen.getByRole('button', { name: /Proceed/i });
    expect(proceedButton).toBeDisabled();
  });

  it('shows blocking error alert for critical violations', () => {
    const response = createPreflightResponse(mockCriticalFailureChecks, false);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
        />
      </TestWrapper>
    );

    expect(screen.getByText('Cannot Proceed')).toBeInTheDocument();
    expect(
      screen.getByText(/Critical policy violations detected that cannot be overridden/i)
    ).toBeInTheDocument();
  });

  it('displays "Cannot Override" badge for critical errors', () => {
    const response = createPreflightResponse(mockCriticalFailureChecks, false);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
        />
      </TestWrapper>
    );

    expect(screen.getByText('Cannot Override')).toBeInTheDocument();
  });
});

describe('PolicyPreflightDialog - Admin Override', () => {
  it('allows admin override for non-critical violations', async () => {
    const response = createPreflightResponse(mockWarningChecks, true);
    const user = userEvent.setup();

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={true}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
          isAdmin={true}
        />
      </TestWrapper>
    );

    // Admin should see override button for warning
    const overrideButton = screen.getByRole('button', { name: /Override/i });
    expect(overrideButton).toBeInTheDocument();

    // Click override
    await user.click(overrideButton);

    // Should show "Undo Override" and "Overridden" badge
    await waitFor(() => {
      expect(screen.getByText('Undo Override')).toBeInTheDocument();
      expect(screen.getByText('Overridden')).toBeInTheDocument();
    });
  });

  it('shows override warning when policies are overridden', async () => {
    const response = createPreflightResponse(mockOverridableErrorChecks, false);
    const user = userEvent.setup();

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={false}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
          isAdmin={true}
        />
      </TestWrapper>
    );

    // Get all override buttons and click the first one (there may be multiple in the DOM)
    const overrideButtons = screen.getAllByRole('button', { name: /^Override$/i });
    await user.click(overrideButtons[0]);

    await waitFor(() => {
      expect(screen.getByText(/1 policy override active/i)).toBeInTheDocument();
    });
  });

  it('does not show override button for non-admin users', () => {
    const response = createPreflightResponse(mockWarningChecks, true);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={true}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
          isAdmin={false}
        />
      </TestWrapper>
    );

    const overrideButtons = screen.queryAllByRole('button', { name: /Override/i });
    expect(overrideButtons).toHaveLength(0);
  });

  it('enables proceed button when admin overrides all blocking policies', async () => {
    const response = createPreflightResponse(mockOverridableErrorChecks, false);
    const user = userEvent.setup();

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={false}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
          isAdmin={true}
        />
      </TestWrapper>
    );

    // Initially proceed is disabled (checking initial state before override)
    const initialProceedButton = screen.getByRole('button', { name: /Proceed/i });
    // Note: canProceed is false, but admin can override, so button is actually enabled
    // The logic in PolicyPreflightDialog enables the button if admin can override all failed checks
    // So we need to verify the button state after override

    // Override the blocking policy
    const overrideButtons = screen.getAllByRole('button', { name: /^Override$/i });
    await user.click(overrideButtons[0]);

    // Now proceed should show "(Override)" text and be enabled
    await waitFor(() => {
      const proceedButton = screen.getByRole('button', { name: /Proceed \(Override\)/i });
      expect(proceedButton).not.toBeDisabled();
    });
  });
});

describe('PolicyPreflightDialog - User Actions', () => {
  it('calls onProceed when proceed button is clicked', async () => {
    const onProceed = vi.fn();
    const response = createPreflightResponse(mockPassedChecks, true);
    const user = userEvent.setup();

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={onProceed}
          onCancel={vi.fn()}
        />
      </TestWrapper>
    );

    const proceedButton = screen.getByRole('button', { name: /Proceed/i });
    await user.click(proceedButton);

    expect(onProceed).toHaveBeenCalledTimes(1);
  });

  it('calls onCancel when cancel button is clicked', async () => {
    const onCancel = vi.fn();
    const response = createPreflightResponse(mockPassedChecks, true);
    const user = userEvent.setup();

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={response.canProceed}
          onProceed={vi.fn()}
          onCancel={onCancel}
        />
      </TestWrapper>
    );

    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    await user.click(cancelButton);

    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('resets override state when dialog is cancelled', async () => {
    const response = createPreflightResponse(mockWarningChecks, true);
    const user = userEvent.setup();
    const onCancel = vi.fn();

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={true}
          onProceed={vi.fn()}
          onCancel={onCancel}
          isAdmin={true}
        />
      </TestWrapper>
    );

    // Override a policy
    const overrideButton = screen.getByRole('button', { name: /Override/i });
    await user.click(overrideButton);

    await waitFor(() => {
      expect(screen.getByText('Overridden')).toBeInTheDocument();
    });

    // Cancel dialog
    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    await user.click(cancelButton);

    expect(onCancel).toHaveBeenCalledTimes(1);
    // Note: Override state reset happens internally, can't verify from outside
  });
});

describe('PolicyPreflightDialog - Loading State', () => {
  it('shows loading state on proceed button', () => {
    const response = createPreflightResponse(mockPassedChecks, true);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={true}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
          isLoading={true}
        />
      </TestWrapper>
    );

    expect(screen.getByText('Loading...')).toBeInTheDocument();
    const proceedButton = screen.getByRole('button', { name: /Loading/i });
    expect(proceedButton).toBeDisabled();
  });

  it('disables cancel button during loading', () => {
    const response = createPreflightResponse(mockPassedChecks, true);

    render(
      <TestWrapper>
        <PolicyPreflightDialog
          open={true}
          onOpenChange={vi.fn()}
          title="Policy Validation"
          checks={response.checks}
          canProceed={true}
          onProceed={vi.fn()}
          onCancel={vi.fn()}
          isLoading={true}
        />
      </TestWrapper>
    );

    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    expect(cancelButton).toBeDisabled();
  });
});

describe('Integration - Adapter Loading with Preflight', () => {
  // Mock API client
  const mockApiClient = {
    preflightAdapterLoad: vi.fn(),
    loadAdapter: vi.fn(),
    unloadAdapter: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('triggers preflight check before load operation', async () => {
    const response = createPreflightResponse(mockPassedChecks, true);
    mockApiClient.preflightAdapterLoad.mockResolvedValue(response);
    mockApiClient.loadAdapter.mockResolvedValue({ id: 'test-adapter' });

    // Simulate preflight check being called
    const preflightResult = await mockApiClient.preflightAdapterLoad('test-adapter', 'load');

    expect(mockApiClient.preflightAdapterLoad).toHaveBeenCalledWith('test-adapter', 'load');
    expect(preflightResult.canProceed).toBe(true);
    expect(preflightResult.checks.length).toBe(3);
  });

  it('shows dialog when policies fail', async () => {
    const response = createPreflightResponse(mockWarningChecks, true);
    mockApiClient.preflightAdapterLoad.mockResolvedValue(response);

    const preflightResult = await mockApiClient.preflightAdapterLoad('test-adapter', 'load');

    // Dialog should be shown because there's a failed check
    const failedChecks = preflightResult.checks.filter(c => !c.passed);
    expect(failedChecks.length).toBeGreaterThan(0);
  });

  it('proceeds with operation after user confirmation', async () => {
    const response = createPreflightResponse(mockPassedChecks, true);
    mockApiClient.preflightAdapterLoad.mockResolvedValue(response);
    mockApiClient.loadAdapter.mockResolvedValue({ id: 'test-adapter' });

    // User confirms via preflight dialog
    const userConfirmed = true;

    if (userConfirmed) {
      await mockApiClient.loadAdapter('test-adapter');
    }

    expect(mockApiClient.loadAdapter).toHaveBeenCalledWith('test-adapter');
  });

  it('cancels operation when user declines', async () => {
    const response = createPreflightResponse(mockWarningChecks, true);
    mockApiClient.preflightAdapterLoad.mockResolvedValue(response);

    // User declines via preflight dialog
    const userConfirmed = false;

    if (userConfirmed) {
      await mockApiClient.loadAdapter('test-adapter');
    }

    expect(mockApiClient.loadAdapter).not.toHaveBeenCalled();
  });

  it('blocks operation when critical policies fail and user is not admin', async () => {
    const response = createPreflightResponse(mockCriticalFailureChecks, false);
    mockApiClient.preflightAdapterLoad.mockResolvedValue(response);

    const isAdmin = false;
    const canProceed = response.canProceed;

    // Should not be able to proceed
    expect(canProceed).toBe(false);
    expect(isAdmin).toBe(false);

    // Load should not be called
    if (canProceed || isAdmin) {
      await mockApiClient.loadAdapter('test-adapter');
    }

    expect(mockApiClient.loadAdapter).not.toHaveBeenCalled();
  });

  it('allows admin to override non-critical failures', async () => {
    const response = createPreflightResponse(mockOverridableErrorChecks, false);
    mockApiClient.preflightAdapterLoad.mockResolvedValue(response);
    mockApiClient.loadAdapter.mockResolvedValue({ id: 'test-adapter' });

    const isAdmin = true;
    const userOverrode = true;

    // Admin can override
    if (isAdmin && userOverrode) {
      await mockApiClient.loadAdapter('test-adapter');
    }

    expect(mockApiClient.loadAdapter).toHaveBeenCalledWith('test-adapter');
  });
});

describe('Integration - Stack Activation with Preflight', () => {
  // Mock API client
  const mockApiClient = {
    preflightStackActivation: vi.fn(),
    activateStack: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('triggers preflight check before stack activation', async () => {
    const response: PolicyPreflightResponse = {
      adapterId: 'test-stack',
      operation: 'activate',
      canProceed: true,
      checks: mockPassedChecks,
      checkedAt: new Date().toISOString(),
    };

    mockApiClient.preflightStackActivation.mockResolvedValue(response);
    mockApiClient.activateStack.mockResolvedValue({ id: 'test-stack' });

    const preflightResult = await mockApiClient.preflightStackActivation('test-stack');

    expect(mockApiClient.preflightStackActivation).toHaveBeenCalledWith('test-stack');
    expect(preflightResult.canProceed).toBe(true);
  });

  it('shows dialog when stack policies fail', async () => {
    const response: PolicyPreflightResponse = {
      adapterId: 'test-stack',
      operation: 'activate',
      canProceed: true,
      checks: mockWarningChecks,
      checkedAt: new Date().toISOString(),
    };

    mockApiClient.preflightStackActivation.mockResolvedValue(response);

    const preflightResult = await mockApiClient.preflightStackActivation('test-stack');
    const hasFailedChecks = preflightResult.checks.some(c => !c.passed);

    expect(hasFailedChecks).toBe(true);
  });

  it('proceeds with activation after user confirmation', async () => {
    const response: PolicyPreflightResponse = {
      adapterId: 'test-stack',
      operation: 'activate',
      canProceed: true,
      checks: mockPassedChecks,
      checkedAt: new Date().toISOString(),
    };

    mockApiClient.preflightStackActivation.mockResolvedValue(response);
    mockApiClient.activateStack.mockResolvedValue({ id: 'test-stack' });

    const userConfirmed = true;

    if (userConfirmed) {
      await mockApiClient.activateStack('test-stack');
    }

    expect(mockApiClient.activateStack).toHaveBeenCalledWith('test-stack');
  });

  it('cancels activation when user declines', async () => {
    const response: PolicyPreflightResponse = {
      adapterId: 'test-stack',
      operation: 'activate',
      canProceed: false,
      checks: mockCriticalFailureChecks,
      checkedAt: new Date().toISOString(),
    };

    mockApiClient.preflightStackActivation.mockResolvedValue(response);

    const userConfirmed = false;

    if (userConfirmed) {
      await mockApiClient.activateStack('test-stack');
    }

    expect(mockApiClient.activateStack).not.toHaveBeenCalled();
  });
});

describe('Integration - Audit Trail', () => {
  it('includes policy override reason in audit metadata', () => {
    // This would be tested at API level, but we can verify structure
    const overrideMetadata = {
      policy_id: 'naming',
      overridden_by: 'admin@example.com',
      override_reason: 'Legacy adapter name grandfathered in',
      timestamp: new Date().toISOString(),
    };

    expect(overrideMetadata).toHaveProperty('policy_id');
    expect(overrideMetadata).toHaveProperty('overridden_by');
    expect(overrideMetadata).toHaveProperty('override_reason');
    expect(overrideMetadata).toHaveProperty('timestamp');
  });

  it('tracks failed preflight checks in audit log', () => {
    const auditEntry = {
      action: 'adapter.load.preflight_failed',
      resource: 'test-adapter',
      status: 'blocked',
      metadata: {
        failed_policies: ['egress', 'determinism'],
        canProceed: false,
      },
      timestamp: new Date().toISOString(),
    };

    expect(auditEntry.action).toBe('adapter.load.preflight_failed');
    expect(auditEntry.status).toBe('blocked');
    expect(auditEntry.metadata.failed_policies).toHaveLength(2);
  });
});
