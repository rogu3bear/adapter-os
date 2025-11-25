import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import GuidedFlowPage from '@/pages/GuidedFlowPage';

const useDatasetMock = vi.fn();
const useTrainingJobMock = vi.fn();
const useCreateDatasetMock = vi.fn();
const useValidateDatasetMock = vi.fn();
const useAdapterStacksMock = vi.fn();

vi.mock('@/hooks/useTraining', () => ({
  useTraining: {
    useDataset: (...args: unknown[]) => useDatasetMock(...args),
    useTrainingJob: (...args: unknown[]) => useTrainingJobMock(...args),
    useCreateDataset: (...args: unknown[]) => useCreateDatasetMock(...args),
    useValidateDataset: (...args: unknown[]) => useValidateDatasetMock(...args),
  },
}));

vi.mock('@/hooks/useAdmin', () => ({
  useAdapterStacks: (...args: unknown[]) => useAdapterStacksMock(...args),
}));

vi.mock('@/components/ChatInterface', () => ({
  ChatInterface: ({ initialStackId }: { initialStackId?: string }) => (
    <div data-testid="chat-interface">Chat stack: {initialStackId || 'default'}</div>
  ),
}));

vi.mock('@/layout/LayoutProvider', () => ({
  useTenant: () => ({ selectedTenant: 'default' }),
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    message: vi.fn(),
  },
}));

describe('GuidedFlowPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useCreateDatasetMock.mockReturnValue({ mutateAsync: vi.fn(), isPending: false });
    useValidateDatasetMock.mockReturnValue({ mutateAsync: vi.fn(), isPending: false });
  });

  it('shows chat using newly trained stack when job is complete', () => {
    useDatasetMock.mockReturnValue({ data: { id: 'ds-1', name: 'Doc', validation_status: 'valid' } });
    useTrainingJobMock.mockReturnValue({
      data: { id: 'job-1', status: 'completed', stack_id: 'stack-42' },
    });
    useAdapterStacksMock.mockReturnValue({ data: [{ id: 'stack-42', name: 'Stack 42' }] });

    render(
      <MemoryRouter>
        <GuidedFlowPage />
      </MemoryRouter>
    );

    expect(screen.getByText(/stack: stack-42/i)).toBeInTheDocument();
    expect(screen.getByTestId('chat-interface')).toHaveTextContent('stack-42');
  });

  it('blocks training when dataset is invalid and surfaces error', () => {
    useDatasetMock.mockReturnValue({
      data: { id: 'ds-2', name: 'Bad Doc', validation_status: 'invalid', validation_errors: 'schema error' },
    });
    useTrainingJobMock.mockReturnValue({ data: undefined });
    useAdapterStacksMock.mockReturnValue({ data: [] });

    render(
      <MemoryRouter>
        <GuidedFlowPage />
      </MemoryRouter>
    );

    expect(screen.getByText(/schema error/i)).toBeInTheDocument();
    expect(screen.getByText(/Start training first to enable chat/i)).toBeInTheDocument();
    expect(screen.queryByTestId('chat-interface')).not.toBeInTheDocument();
  });
});
