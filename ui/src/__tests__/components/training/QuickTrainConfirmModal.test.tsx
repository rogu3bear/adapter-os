/**
 * Tests for QuickTrainConfirmModal component
 *
 * Covers:
 * - Modal rendering
 * - Form validation (adapter name)
 * - Submit behavior
 * - Cancel behavior
 * - Loading states
 * - Error handling (preflight checks)
 * - Advanced settings toggle
 * - Dataset summary display
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { QuickTrainConfirmModal } from '@/components/training/QuickTrainConfirmModal';
import type { Dataset } from '@/api/training-types';
import type { QuickTrainConfig } from '@/components/training/QuickTrainConfirmModal';

// Mock useTrainingPreflight hook
const mockUseTrainingPreflight = vi.fn();

vi.mock('@/hooks/training', () => ({
  useTrainingPreflight: (dataset: Dataset, options: any) => mockUseTrainingPreflight(dataset, options),
}));

// Test wrapper
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <QueryClientProvider client={queryClient}>
      {children}
    </QueryClientProvider>
  );
}

// Mock data
const mockDataset: Dataset = {
  id: 'dataset-123',
  name: 'Test Dataset',
  file_count: 42,
  total_size_bytes: 1024 * 1024 * 5, // 5 MB
  total_tokens: 50000,
  validation_status: 'valid',
  trust_state: 'allowed',
  created_at: '2025-01-01T10:00:00Z',
  updated_at: '2025-01-01T11:00:00Z',
};

const mockPreflightSuccess = {
  canProceed: true,
  isClean: true,
  clientChecks: [
    {
      policy_id: 'validation_status',
      policy_name: 'Dataset Validation',
      passed: true,
      severity: 'info' as const,
      message: 'Dataset is valid',
    },
  ],
  serverChecks: [
    {
      policy_id: 'worker_available',
      policy_name: 'Worker Available',
      passed: true,
      severity: 'info' as const,
      message: 'Model ready',
    },
  ],
  allChecks: [],
  summary: 'All checks passed. Ready to start training.',
  isLoading: false,
  error: null,
  refetch: vi.fn(),
};

const mockPreflightFailure = {
  ...mockPreflightSuccess,
  canProceed: false,
  isClean: false,
  clientChecks: [
    {
      policy_id: 'validation_status',
      policy_name: 'Dataset Validation',
      passed: false,
      severity: 'error' as const,
      message: 'Dataset validation failed',
      details: 'Dataset contains invalid entries',
    },
  ],
  allChecks: [],
  summary: '1 issue must be resolved.',
};

describe('QuickTrainConfirmModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseTrainingPreflight.mockReturnValue({
      ...mockPreflightSuccess,
      allChecks: [...mockPreflightSuccess.clientChecks, ...mockPreflightSuccess.serverChecks],
    });
  });

  describe('rendering', () => {
    it('renders modal when open', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByTestId('quick-train-modal')).toBeInTheDocument();
      // Dialog title
      expect(screen.getAllByText('Start Training')[0]).toBeInTheDocument();
    });

    it('does not render modal when closed', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={false}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.queryByTestId('quick-train-modal')).not.toBeInTheDocument();
    });

    it('displays dataset summary', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText('Test Dataset')).toBeInTheDocument();
      expect(screen.getByText('42 files')).toBeInTheDocument();
      expect(screen.getByText('5.0 MB')).toBeInTheDocument();
      expect(screen.getByText('50,000 tokens')).toBeInTheDocument();
    });

    it('displays validation status badge', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText('valid')).toBeInTheDocument();
    });

    it('displays trust state badge', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText('allowed')).toBeInTheDocument();
    });

    it('shows all configuration fields', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByTestId('quick-train-adapter-name')).toBeInTheDocument();
      expect(screen.getByTestId('quick-train-rank')).toBeInTheDocument();
      expect(screen.getByTestId('quick-train-alpha')).toBeInTheDocument();
      expect(screen.getByTestId('quick-train-epochs')).toBeInTheDocument();
    });

    it('generates default adapter name from dataset', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const nameInput = screen.getByTestId('quick-train-adapter-name') as HTMLInputElement;
      expect(nameInput.value).toBe('test-dataset-adapter');
    });

    it('sanitizes dataset name for adapter name', () => {
      const datasetWithSpecialChars = {
        ...mockDataset,
        name: 'Test Dataset! @#$ 123',
      };

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={datasetWithSpecialChars}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const nameInput = screen.getByTestId('quick-train-adapter-name') as HTMLInputElement;
      expect(nameInput.value).toBe('test-dataset-123-adapter');
    });
  });

  describe('form validation', () => {
    it('validates adapter name is at least 3 characters', async () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const nameInput = screen.getByTestId('quick-train-adapter-name');

      await user.clear(nameInput);
      await user.type(nameInput, 'ab');

      expect(screen.getByText(/Name must be 3\+ chars/)).toBeInTheDocument();
      expect(screen.getByTestId('quick-train-start')).toBeDisabled();
    });

    it('validates adapter name format', async () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const nameInput = screen.getByTestId('quick-train-adapter-name');

      await user.clear(nameInput);
      await user.type(nameInput, 'Invalid Name!');

      expect(screen.getByText(/lowercase alphanumeric with hyphens/)).toBeInTheDocument();
      expect(screen.getByTestId('quick-train-start')).toBeDisabled();
    });

    it('accepts valid adapter names', async () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const nameInput = screen.getByTestId('quick-train-adapter-name');

      await user.clear(nameInput);
      await user.type(nameInput, 'my-valid-adapter-123');

      expect(screen.queryByText(/Name must be 3\+ chars/)).not.toBeInTheDocument();
      expect(screen.getByTestId('quick-train-start')).not.toBeDisabled();
    });

    it('converts uppercase to lowercase automatically', async () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const nameInput = screen.getByTestId('quick-train-adapter-name') as HTMLInputElement;

      await user.clear(nameInput);
      await user.type(nameInput, 'MyAdapter');

      expect(nameInput.value).toBe('myadapter');
    });

    it('disables start button when preflight checks fail', () => {
      mockUseTrainingPreflight.mockReturnValue({
        ...mockPreflightFailure,
        allChecks: mockPreflightFailure.clientChecks,
      });

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByTestId('quick-train-start')).toBeDisabled();
    });

    it('disables start button when loading', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
            isLoading={true}
          />
        </TestWrapper>
      );

      expect(screen.getByTestId('quick-train-start')).toBeDisabled();
    });
  });

  describe('submit behavior', () => {
    it('calls onConfirm with correct config', async () => {
      const onConfirm = vi.fn();

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={onConfirm}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const startButton = screen.getByTestId('quick-train-start');
      await user.click(startButton);

      expect(onConfirm).toHaveBeenCalledWith({
        adapterName: 'test-dataset-adapter',
        rank: 8,
        alpha: 16,
        epochs: 3,
        learningRate: 3e-4,
        batchSize: 4,
        targets: ['q_proj', 'v_proj'],
      });
    });

    it('submits with custom values', async () => {
      const onConfirm = vi.fn();

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={onConfirm}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Change adapter name
      const nameInput = screen.getByTestId('quick-train-adapter-name');
      await user.clear(nameInput);
      await user.type(nameInput, 'custom-adapter');

      // Submit
      const startButton = screen.getByTestId('quick-train-start');
      await user.click(startButton);

      await waitFor(() => {
        expect(onConfirm).toHaveBeenCalled();
        const call = onConfirm.mock.calls[0][0];
        expect(call.adapterName).toBe('custom-adapter');
      });
    });

    it('does not submit when form is invalid', async () => {
      const onConfirm = vi.fn();

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={onConfirm}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Set invalid name
      const nameInput = screen.getByTestId('quick-train-adapter-name');
      await user.clear(nameInput);
      await user.type(nameInput, 'ab');

      const startButton = screen.getByTestId('quick-train-start');
      await user.click(startButton);

      expect(onConfirm).not.toHaveBeenCalled();
    });
  });

  describe('cancel behavior', () => {
    it('calls onCancel when cancel button is clicked', async () => {
      const onCancel = vi.fn();

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={onCancel}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const cancelButton = screen.getByTestId('quick-train-cancel');
      await user.click(cancelButton);

      expect(onCancel).toHaveBeenCalled();
    });

    it('disables cancel button when loading', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
            isLoading={true}
          />
        </TestWrapper>
      );

      expect(screen.getByTestId('quick-train-cancel')).toBeDisabled();
    });

    it('calls onAdvanced when Advanced button is clicked', async () => {
      const onAdvanced = vi.fn();

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
            onAdvanced={onAdvanced}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const advancedButton = screen.getByTestId('quick-train-advanced-btn');
      await user.click(advancedButton);

      expect(onAdvanced).toHaveBeenCalled();
    });

    it('resets advanced settings state when cancelled', async () => {
      const onCancel = vi.fn();

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={onCancel}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Open advanced settings
      const advancedToggle = screen.getByRole('button', { name: /Advanced Options/i });
      await user.click(advancedToggle);

      // Cancel
      const cancelButton = screen.getByTestId('quick-train-cancel');
      await user.click(cancelButton);

      expect(onCancel).toHaveBeenCalled();
    });
  });

  describe('loading states', () => {
    it('shows loading text when isLoading is true', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
            isLoading={true}
          />
        </TestWrapper>
      );

      expect(screen.getByText('Starting...')).toBeInTheDocument();
    });

    it('shows loading spinner when isLoading is true', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
            isLoading={true}
          />
        </TestWrapper>
      );

      const startButton = screen.getByTestId('quick-train-start');
      expect(startButton.querySelector('svg')).toBeInTheDocument();
    });

    it('shows preflight loading state', () => {
      mockUseTrainingPreflight.mockReturnValue({
        ...mockPreflightSuccess,
        isLoading: true,
        allChecks: [],
      });

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const preflightSection = screen.getByText('Preflight Checks').closest('div');
      expect(preflightSection?.querySelector('svg')).toBeInTheDocument(); // Loading spinner
    });
  });

  describe('error handling', () => {
    it('displays failed preflight checks', () => {
      mockUseTrainingPreflight.mockReturnValue({
        ...mockPreflightFailure,
        allChecks: mockPreflightFailure.clientChecks,
      });

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText('Dataset Validation')).toBeInTheDocument();
      expect(screen.getByText('Dataset validation failed')).toBeInTheDocument();
      expect(screen.getByText('Dataset contains invalid entries')).toBeInTheDocument();
    });

    it('displays warning checks with correct styling', () => {
      const preflightWithWarning = {
        ...mockPreflightSuccess,
        isClean: false,
        clientChecks: [
          {
            policy_id: 'size_warning',
            policy_name: 'Dataset Size',
            passed: true,
            severity: 'warning' as const,
            message: 'Dataset is large',
            details: 'This may take longer to train.',
          },
        ],
        allChecks: [],
      };

      mockUseTrainingPreflight.mockReturnValue({
        ...preflightWithWarning,
        allChecks: preflightWithWarning.clientChecks,
      });

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText('Dataset Size')).toBeInTheDocument();
      expect(screen.getByText('Dataset is large')).toBeInTheDocument();
    });

    it('shows success badge when all checks pass', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByTestId('quick-train-preflight-passed')).toBeInTheDocument();
      expect(screen.getByText('All checks passed')).toBeInTheDocument();
    });
  });

  describe('advanced settings', () => {
    it('hides advanced settings by default', () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.queryByLabelText('Learning Rate')).not.toBeInTheDocument();
      expect(screen.queryByLabelText('Batch Size')).not.toBeInTheDocument();
    });

    it('shows advanced settings when toggled', async () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const advancedToggle = screen.getByRole('button', { name: /Advanced Options/i });
      await user.click(advancedToggle);

      expect(screen.getByLabelText('Learning Rate')).toBeInTheDocument();
      expect(screen.getByLabelText('Batch Size')).toBeInTheDocument();
    });

    it('allows changing advanced settings', async () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Open advanced settings
      const advancedToggle = screen.getByRole('button', { name: /Advanced Options/i });
      await user.click(advancedToggle);

      // Verify fields are accessible
      expect(screen.getByLabelText('Learning Rate')).toBeInTheDocument();
      expect(screen.getByLabelText('Batch Size')).toBeInTheDocument();
    });

    it('shows link to full training wizard in advanced section', async () => {
      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={mockDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
            onAdvanced={vi.fn()}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const advancedToggle = screen.getByRole('button', { name: /Advanced Options/i });
      await user.click(advancedToggle);

      expect(screen.getByText(/full training wizard/)).toBeInTheDocument();
    });
  });

  describe('dataset formatting', () => {
    it('formats large file sizes correctly', () => {
      const largeDataset = {
        ...mockDataset,
        total_size_bytes: 1024 * 1024 * 1024 * 2.5, // 2.5 GB
      };

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={largeDataset}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText('2.50 GB')).toBeInTheDocument();
    });

    it('hides token count when zero', () => {
      const datasetNoTokens = {
        ...mockDataset,
        total_tokens: 0,
      };

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={datasetNoTokens}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.queryByText(/tokens/)).not.toBeInTheDocument();
    });

    it('shows different badge variants for trust states', () => {
      const datasetWithWarning = {
        ...mockDataset,
        trust_state: 'allowed_with_warning' as const,
      };

      render(
        <TestWrapper>
          <QuickTrainConfirmModal
            open={true}
            onOpenChange={vi.fn()}
            dataset={datasetWithWarning}
            onConfirm={vi.fn()}
            onCancel={vi.fn()}
          />
        </TestWrapper>
      );

      expect(screen.getByText('allowed_with_warning')).toBeInTheDocument();
    });
  });
});
