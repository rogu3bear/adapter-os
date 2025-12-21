/**
 * Tests for QuickTrainConfirmModal component
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import QuickTrainConfirmModal, {
  type QuickTrainConfig,
} from '@/components/training/QuickTrainConfirmModal';
import type { Dataset } from '@/api/training-types';

// Mock the useTrainingPreflight hook
vi.mock('@/hooks/training', () => ({
  useTrainingPreflight: vi.fn().mockReturnValue({
    isLoading: false,
    canProceed: true,
    allChecks: [
      {
        policy_id: 'validation_status',
        policy_name: 'Dataset Validated',
        passed: true,
        severity: 'info',
        message: 'Dataset has passed validation',
      },
      {
        policy_id: 'trust_state',
        policy_name: 'Trust State',
        passed: true,
        severity: 'info',
        message: 'Trust: allowed',
      },
      {
        policy_id: 'file_count',
        policy_name: 'Files Present',
        passed: true,
        severity: 'info',
        message: '10 files in dataset',
      },
      {
        policy_id: 'size_limit',
        policy_name: 'Dataset Size',
        passed: true,
        severity: 'info',
        message: 'Size: 1.0 MB',
      },
    ],
    clientChecks: [],
    serverChecks: [],
    clientPassed: true,
    serverPassed: true,
    hasErrors: false,
    hasWarnings: false,
    summary: 'All checks passed. Ready to start training.',
  }),
}));

/**
 * Create a mock dataset with sensible defaults
 */
function createMockDataset(overrides: Partial<Dataset> = {}): Dataset {
  return {
    id: 'test-dataset-id',
    name: 'Test Training Dataset',
    description: 'A test dataset',
    file_count: 10,
    total_size_bytes: 1024 * 1024, // 1 MB
    format: 'jsonl',
    hash_b3: 'abc123',
    storage_path: '/test/path',
    validation_status: 'valid',
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
    trust_state: 'allowed',
    total_tokens: 10000,
    ...overrides,
  } as Dataset;
}

describe('QuickTrainConfirmModal', () => {
  const mockOnOpenChange = vi.fn();
  const mockOnConfirm = vi.fn();
  const mockOnCancel = vi.fn();
  const mockOnAdvanced = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders dialog with title and description when open', () => {
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    expect(screen.getByRole('heading', { name: /Start Training/ })).toBeInTheDocument();
    expect(screen.getByText('Train a new adapter from the selected dataset.')).toBeInTheDocument();
  });

  it('displays dataset summary information', () => {
    const dataset = createMockDataset({
      name: 'My Test Dataset',
      file_count: 25,
      total_size_bytes: 5 * 1024 * 1024, // 5 MB
      total_tokens: 50000,
    });

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    expect(screen.getByText('My Test Dataset')).toBeInTheDocument();
    expect(screen.getByText('25 files')).toBeInTheDocument();
    // formatBytes uses 2 decimal places for values < 10
    expect(screen.getByText('5.00 MB')).toBeInTheDocument();
    expect(screen.getByText('50,000 tokens')).toBeInTheDocument();
  });

  it('displays validation and trust badges', () => {
    const dataset = createMockDataset({
      validation_status: 'valid',
      trust_state: 'allowed',
    });

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    expect(screen.getByText('valid')).toBeInTheDocument();
    expect(screen.getByText('allowed')).toBeInTheDocument();
  });

  it('shows preflight checks section', () => {
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    expect(screen.getByText('Preflight Checks')).toBeInTheDocument();
    expect(screen.getByText('All checks passed')).toBeInTheDocument();
  });

  it('generates default adapter name from dataset name', () => {
    const dataset = createMockDataset({ name: 'My Training Data' });

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    const adapterNameInput = screen.getByLabelText('Adapter Name') as HTMLInputElement;
    expect(adapterNameInput.value).toBe('my-training-data-adapter');
  });

  it('allows editing adapter name', async () => {
    const user = userEvent.setup();
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    const adapterNameInput = screen.getByLabelText('Adapter Name') as HTMLInputElement;
    await user.clear(adapterNameInput);
    await user.type(adapterNameInput, 'custom-adapter');

    expect(adapterNameInput.value).toBe('custom-adapter');
  });

  it('shows validation error for invalid adapter name', async () => {
    const user = userEvent.setup();
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    const adapterNameInput = screen.getByLabelText('Adapter Name') as HTMLInputElement;
    await user.clear(adapterNameInput);
    await user.type(adapterNameInput, 'ab'); // Too short

    expect(
      screen.getByText(/Name must be 3\+ chars, lowercase alphanumeric with hyphens/)
    ).toBeInTheDocument();
  });

  it('renders configuration inputs with default values', () => {
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    const rankInput = screen.getByLabelText('Rank') as HTMLInputElement;
    const alphaInput = screen.getByLabelText('Alpha') as HTMLInputElement;
    const epochsInput = screen.getByLabelText('Epochs') as HTMLInputElement;

    expect(rankInput.value).toBe('8');
    expect(alphaInput.value).toBe('16');
    expect(epochsInput.value).toBe('3');
  });

  it('allows modifying training parameters', async () => {
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    const epochsInput = screen.getByLabelText('Epochs') as HTMLInputElement;
    // Verify input is not disabled and can be modified
    expect(epochsInput).not.toBeDisabled();
    expect(epochsInput.type).toBe('number');

    // Simulate a change event directly
    fireEvent.change(epochsInput, { target: { value: '5' } });
    expect(epochsInput.value).toBe('5');
  });

  it('toggles advanced options', async () => {
    const user = userEvent.setup();
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    // Advanced options should be collapsed by default
    expect(screen.queryByLabelText('Learning Rate')).not.toBeInTheDocument();

    // Click to expand
    const advancedButton = screen.getByText('Advanced Options');
    await user.click(advancedButton);

    // Should now show advanced options
    await waitFor(() => {
      expect(screen.getByLabelText('Learning Rate')).toBeInTheDocument();
      expect(screen.getByLabelText('Batch Size')).toBeInTheDocument();
    });
  });

  it('calls onConfirm with config when Start Training is clicked', async () => {
    const user = userEvent.setup();
    const dataset = createMockDataset({ name: 'test-data' });

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    const startButton = screen.getByRole('button', { name: /Start Training/ });
    await user.click(startButton);

    expect(mockOnConfirm).toHaveBeenCalledTimes(1);
    const calledConfig: QuickTrainConfig = mockOnConfirm.mock.calls[0][0];
    expect(calledConfig.adapterName).toBe('test-data-adapter');
    expect(calledConfig.rank).toBe(8);
    expect(calledConfig.alpha).toBe(16);
    expect(calledConfig.epochs).toBe(3);
  });

  it('calls onCancel when Cancel is clicked', async () => {
    const user = userEvent.setup();
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    const cancelButton = screen.getByRole('button', { name: 'Cancel' });
    await user.click(cancelButton);

    expect(mockOnCancel).toHaveBeenCalledTimes(1);
  });

  it('calls onAdvanced when Advanced button is clicked', async () => {
    const user = userEvent.setup();
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
        onAdvanced={mockOnAdvanced}
      />
    );

    const advancedButton = screen.getByRole('button', { name: 'Advanced...' });
    await user.click(advancedButton);

    expect(mockOnAdvanced).toHaveBeenCalledTimes(1);
  });

  it('disables Start Training button when loading', () => {
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
        isLoading={true}
      />
    );

    const startButton = screen.getByRole('button', { name: /Starting.../ });
    expect(startButton).toBeDisabled();
  });

  it('shows loading indicator when isLoading is true', () => {
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={true}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
        isLoading={true}
      />
    );

    expect(screen.getByText('Starting...')).toBeInTheDocument();
  });

  it('does not render when open is false', () => {
    const dataset = createMockDataset();

    render(
      <QuickTrainConfirmModal
        open={false}
        onOpenChange={mockOnOpenChange}
        dataset={dataset}
        onConfirm={mockOnConfirm}
        onCancel={mockOnCancel}
      />
    );

    expect(screen.queryByRole('heading', { name: /Start Training/ })).not.toBeInTheDocument();
  });
});
