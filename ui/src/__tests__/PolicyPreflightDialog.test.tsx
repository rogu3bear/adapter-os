/**
 * Tests for PolicyPreflightDialog component
 *
 * Citation: [2025-11-25†ui†policy-preflight-dialog-tests]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { PolicyPreflightDialog, PolicyCheck } from '@/components/PolicyPreflightDialog';

describe('PolicyPreflightDialog', () => {
  const mockChecks: PolicyCheck[] = [
    {
      policy_id: 'egress-001',
      policy_name: 'Zero Network Egress',
      passed: true,
      severity: 'error',
      message: 'No network egress detected in production mode',
    },
    {
      policy_id: 'determinism-001',
      policy_name: 'Deterministic Execution',
      passed: false,
      severity: 'error',
      message: 'Non-deterministic randomness detected',
      can_override: false,
    },
    {
      policy_id: 'naming-001',
      policy_name: 'Semantic Naming',
      passed: false,
      severity: 'warning',
      message: 'Adapter name does not follow semantic convention',
      can_override: true,
    },
  ];

  it('should render dialog with title and description', () => {
    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        description="Review policy checks before proceeding"
        checks={mockChecks}
        canProceed={false}
        onProceed={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    expect(screen.getByText('Policy Validation')).toBeInTheDocument();
    expect(screen.getByText('Review policy checks before proceeding')).toBeInTheDocument();
  });

  it('should display correct statistics', () => {
    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={mockChecks}
        canProceed={false}
        onProceed={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    // Total: 3, Passed: 1, Errors: 1 (determinism failed), Warnings: 1 (naming failed)
    // Check for statistics in the summary section
    const totalSection = screen.getByText('Total').parentElement;
    expect(totalSection?.textContent).toContain('3');

    const passedSection = screen.getByText('Passed').parentElement;
    expect(passedSection?.textContent).toContain('1');

    const errorsSection = screen.getByText('Errors').parentElement;
    expect(errorsSection?.textContent).toContain('1');

    const warningsSection = screen.getByText('Warnings').parentElement;
    expect(warningsSection?.textContent).toContain('1');
  });

  it('should show blocking error alert when critical policies fail', () => {
    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={mockChecks}
        canProceed={false}
        onProceed={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    expect(screen.getByText('Cannot Proceed')).toBeInTheDocument();
    expect(screen.getByText(/Critical policy violations detected/)).toBeInTheDocument();
  });

  it('should disable proceed button when blocking errors exist', () => {
    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={mockChecks}
        canProceed={false}
        onProceed={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    const proceedButton = screen.getByRole('button', { name: /Proceed/ });
    expect(proceedButton).toBeDisabled();
  });

  it('should show override button for admin users on overridable policies', () => {
    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={mockChecks}
        canProceed={false}
        onProceed={vi.fn()}
        onCancel={vi.fn()}
        isAdmin={true}
      />
    );

    // Should show override button for warning (can_override: true)
    const overrideButtons = screen.getAllByRole('button', { name: /Override/ });
    expect(overrideButtons.length).toBeGreaterThan(0);
  });

  it('should not show override button for non-admin users', () => {
    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={mockChecks}
        canProceed={false}
        onProceed={vi.fn()}
        onCancel={vi.fn()}
        isAdmin={false}
      />
    );

    // Non-admin should not see override buttons
    // Use more specific query to avoid matching other buttons
    const overrideButtons = screen.queryAllByRole('button', { name: /^Override$/i });
    expect(overrideButtons).toHaveLength(0);
  });

  it('should call onProceed when proceed button is clicked', () => {
    const onProceed = vi.fn();

    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={[mockChecks[0]]} // Only passed check
        canProceed={true}
        onProceed={onProceed}
        onCancel={vi.fn()}
      />
    );

    const proceedButton = screen.getByRole('button', { name: /Proceed/ });
    fireEvent.click(proceedButton);

    expect(onProceed).toHaveBeenCalledTimes(1);
  });

  it('should call onCancel when cancel button is clicked', () => {
    const onCancel = vi.fn();

    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={mockChecks}
        canProceed={false}
        onProceed={vi.fn()}
        onCancel={onCancel}
      />
    );

    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    fireEvent.click(cancelButton);

    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('should display passed checks in collapsed section', () => {
    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={mockChecks}
        canProceed={false}
        onProceed={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    // Should have a details/summary for passed checks
    expect(screen.getByText(/Passed Checks \(1\)/)).toBeInTheDocument();
  });

  it('should show loading state on proceed button', () => {
    render(
      <PolicyPreflightDialog
        open={true}
        onOpenChange={vi.fn()}
        title="Policy Validation"
        checks={[mockChecks[0]]}
        canProceed={true}
        onProceed={vi.fn()}
        onCancel={vi.fn()}
        isLoading={true}
      />
    );

    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });
});
