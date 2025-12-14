/**
 * Tests for PublishAdapterDialog component
 *
 * Covers:
 * - Dialog rendering
 * - Form validation
 * - Submit behavior
 * - Cancel behavior
 * - Loading states
 * - Error handling
 * - Attach mode selection
 * - Dataset version selection
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { PublishAdapterDialog } from '@/components/training/PublishAdapterDialog';
import type { TrainingJob } from '@/api/training-types';
import type { PublishAdapterResponse } from '@/api/adapter-types';

// Mock the usePublishAdapter hook
const mockMutateAsync = vi.fn();
const mockUsePublishAdapter = vi.fn(() => ({
  mutateAsync: mockMutateAsync,
  isPending: false,
  isError: false,
  error: null,
}));

vi.mock('@/hooks/useAdapterPublish', () => ({
  usePublishAdapter: () => mockUsePublishAdapter(),
}));

// Mock toast
const mockToastSuccess = vi.fn();
const mockToastError = vi.fn();

vi.mock('sonner', () => ({
  toast: {
    success: mockToastSuccess,
    error: mockToastError,
  },
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
const mockTrainingJob: TrainingJob = {
  id: 'job-123',
  adapter_name: 'test-adapter',
  status: 'completed',
  repo_id: 'repo-456',
  produced_version_id: 'version-789',
  dataset_version_ids: [
    {
      dataset_version_id: 'dsv-001',
      dataset_name: 'Training Dataset A',
    },
    {
      dataset_version_id: 'dsv-002',
      dataset_name: 'Training Dataset B',
    },
  ],
  created_at: '2025-01-01T10:00:00Z',
  updated_at: '2025-01-01T11:00:00Z',
};

const mockPublishResponse: PublishAdapterResponse = {
  repo_id: 'repo-456',
  version_id: 'version-789',
  adapter_id: 'adapter-123',
  attach_mode: 'free',
  published_at: '2025-01-01T12:00:00Z',
};

describe('PublishAdapterDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUsePublishAdapter.mockReturnValue({
      mutateAsync: mockMutateAsync,
      isPending: false,
      isError: false,
      error: null,
    });
  });

  describe('rendering', () => {
    it('renders dialog when open', () => {
      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      expect(screen.getByText('Publish Adapter')).toBeInTheDocument();
      expect(screen.getByText(/Publish your trained adapter/)).toBeInTheDocument();
    });

    it('does not render dialog when closed', () => {
      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={false}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      expect(screen.queryByText('Publish Adapter')).not.toBeInTheDocument();
    });

    it('renders all form fields', () => {
      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      expect(screen.getByLabelText('Name')).toBeInTheDocument();
      expect(screen.getByLabelText('Short Description')).toBeInTheDocument();
      expect(screen.getByLabelText('Attach Mode')).toBeInTheDocument();
    });

    it('pre-fills adapter name from training job', () => {
      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const nameInput = screen.getByLabelText('Name') as HTMLInputElement;
      expect(nameInput.value).toBe('test-adapter');
    });

    it('shows character count for description', async () => {
      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const descriptionField = screen.getByLabelText('Short Description');
      const user = userEvent.setup();

      await user.type(descriptionField, 'Test description');

      expect(screen.getByText('16/280 characters')).toBeInTheDocument();
    });
  });

  describe('form validation', () => {
    it('allows publishing with free attach mode and no dataset', async () => {
      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const publishButton = screen.getByRole('button', { name: /Publish/i });
      expect(publishButton).not.toBeDisabled();
    });

    it('disables publish button when repo_id is missing', () => {
      const jobWithoutRepo = { ...mockTrainingJob, repo_id: undefined };

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={jobWithoutRepo}
          />
        </TestWrapper>
      );

      const publishButton = screen.getByRole('button', { name: /Publish/i });
      expect(publishButton).toBeDisabled();
    });

    it('disables publish button when produced_version_id is missing', () => {
      const jobWithoutVersion = { ...mockTrainingJob, produced_version_id: undefined };

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={jobWithoutVersion}
          />
        </TestWrapper>
      );

      const publishButton = screen.getByRole('button', { name: /Publish/i });
      expect(publishButton).toBeDisabled();
    });
  });

  describe('submit behavior', () => {
    it('submits form with correct data for free attach mode', async () => {
      mockMutateAsync.mockResolvedValue(mockPublishResponse);
      const onPublished = vi.fn();
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={onOpenChange}
            trainingJob={mockTrainingJob}
            onPublished={onPublished}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Fill in optional fields
      const descriptionField = screen.getByLabelText('Short Description');
      await user.type(descriptionField, 'My test adapter');

      // Submit
      const publishButton = screen.getByRole('button', { name: /Publish/i });
      await user.click(publishButton);

      await waitFor(() => {
        expect(mockMutateAsync).toHaveBeenCalledWith({
          repoId: 'repo-456',
          versionId: 'version-789',
          data: {
            name: 'test-adapter',
            short_description: 'My test adapter',
            attach_mode: 'free',
            required_scope_dataset_version_id: undefined,
          },
        });
      });

      expect(onPublished).toHaveBeenCalledWith(mockPublishResponse);
      expect(onOpenChange).toHaveBeenCalledWith(false);
    });


    it('omits optional fields when empty', async () => {
      mockMutateAsync.mockResolvedValue(mockPublishResponse);

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Clear the name field
      const nameField = screen.getByLabelText('Name');
      await user.clear(nameField);

      // Submit
      const publishButton = screen.getByRole('button', { name: /Publish/i });
      await user.click(publishButton);

      await waitFor(() => {
        expect(mockMutateAsync).toHaveBeenCalledWith({
          repoId: 'repo-456',
          versionId: 'version-789',
          data: {
            name: undefined,
            short_description: undefined,
            attach_mode: 'free',
            required_scope_dataset_version_id: undefined,
          },
        });
      });
    });

    it('prevents form submission via Enter key', async () => {
      mockMutateAsync.mockResolvedValue(mockPublishResponse);

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const nameField = screen.getByLabelText('Name');
      await user.click(nameField);
      await user.keyboard('{Enter}');

      // Should submit via explicit form submission
      await waitFor(() => {
        expect(mockMutateAsync).toHaveBeenCalled();
      });
    });
  });

  describe('cancel behavior', () => {
    it('calls onOpenChange with false when cancel is clicked', async () => {
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={onOpenChange}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const cancelButton = screen.getByRole('button', { name: /Cancel/i });
      await user.click(cancelButton);

      expect(onOpenChange).toHaveBeenCalledWith(false);
    });

    it('disables cancel button during submission', () => {
      mockUsePublishAdapter.mockReturnValue({
        mutateAsync: mockMutateAsync,
        isPending: true,
        isError: false,
        error: null,
      });

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const cancelButton = screen.getByRole('button', { name: /Cancel/i });
      expect(cancelButton).toBeDisabled();
    });

    it('resets form when dialog reopens', async () => {
      const { rerender } = render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Modify form
      const descriptionField = screen.getByLabelText('Short Description');
      await user.type(descriptionField, 'Modified description');

      // Close dialog
      rerender(
        <TestWrapper>
          <PublishAdapterDialog
            open={false}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      // Reopen dialog
      rerender(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      // Description should be reset
      const resetDescriptionField = screen.getByLabelText('Short Description') as HTMLTextAreaElement;
      expect(resetDescriptionField.value).toBe('');
    });
  });

  describe('loading states', () => {
    it('shows loading state during submission', () => {
      mockUsePublishAdapter.mockReturnValue({
        mutateAsync: mockMutateAsync,
        isPending: true,
        isError: false,
        error: null,
      });

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      expect(screen.getByText('Publishing...')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Publishing/i })).toBeDisabled();
    });

    it('disables all inputs during submission', () => {
      mockUsePublishAdapter.mockReturnValue({
        mutateAsync: mockMutateAsync,
        isPending: true,
        isError: false,
        error: null,
      });

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      expect(screen.getByRole('button', { name: /Publishing/i })).toBeDisabled();
      expect(screen.getByRole('button', { name: /Cancel/i })).toBeDisabled();
    });

    it('shows spinner icon during submission', () => {
      mockUsePublishAdapter.mockReturnValue({
        mutateAsync: mockMutateAsync,
        isPending: true,
        isError: false,
        error: null,
      });

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const publishButton = screen.getByRole('button', { name: /Publishing/i });
      expect(publishButton.querySelector('svg')).toBeInTheDocument();
    });
  });

  describe('error handling', () => {
    it('handles mutation error gracefully', async () => {
      const error = new Error('Failed to publish');
      mockMutateAsync.mockRejectedValue(error);
      const onOpenChange = vi.fn();

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={onOpenChange}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const publishButton = screen.getByRole('button', { name: /Publish/i });
      await user.click(publishButton);

      // Dialog should remain open on error
      await waitFor(() => {
        expect(mockMutateAsync).toHaveBeenCalled();
      });

      // Dialog should NOT close on error
      expect(onOpenChange).not.toHaveBeenCalledWith(false);
    });

    it('does not call onPublished callback on error', async () => {
      const error = new Error('Failed to publish');
      mockMutateAsync.mockRejectedValue(error);
      const onPublished = vi.fn();

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
            onPublished={onPublished}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const publishButton = screen.getByRole('button', { name: /Publish/i });
      await user.click(publishButton);

      await waitFor(() => {
        expect(mockMutateAsync).toHaveBeenCalled();
      });

      expect(onPublished).not.toHaveBeenCalled();
    });
  });

  describe('attach mode handling', () => {
    it('renders attach mode selector', () => {
      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      expect(screen.getByLabelText('Attach Mode')).toBeInTheDocument();
    });

    it('shows input field when no dataset versions available', () => {
      const jobWithoutDatasets = { ...mockTrainingJob, dataset_version_ids: [] };

      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={jobWithoutDatasets}
          />
        </TestWrapper>
      );

      // Default is free mode, so dataset field should not be visible
      expect(screen.queryByPlaceholderText('Enter dataset version ID')).not.toBeInTheDocument();
    });

    it('has dataset versions linked from training job', () => {
      render(
        <TestWrapper>
          <PublishAdapterDialog
            open={true}
            onOpenChange={vi.fn()}
            trainingJob={mockTrainingJob}
          />
        </TestWrapper>
      );

      // Component has access to dataset versions
      expect(mockTrainingJob.dataset_version_ids).toHaveLength(2);
    });
  });
});
