import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TrainingWizard } from '@/components/TrainingWizard';

const listDatasetsMock = vi.fn();
const startTrainingMock = vi.fn();

vi.mock('@/api/client', () => ({
  __esModule: true,
  default: {
    listRepositories: vi.fn().mockResolvedValue([]),
    listTrainingTemplates: vi.fn().mockResolvedValue([]),
    listDatasets: (...args: unknown[]) => listDatasetsMock(...args),
    startTraining: (...args: unknown[]) => startTrainingMock(...args),
  },
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('@/utils/logger', () => ({
  logger: {
    error: vi.fn(),
    warn: vi.fn(),
  },
  toError: (error: unknown) => error,
}));

describe('TrainingWizard dataset validation guard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('blocks training when selected dataset is not validated', async () => {
    listDatasetsMock.mockResolvedValue({
      datasets: [{ id: 'ds-draft', name: 'Draft Dataset', validation_status: 'draft' }],
    });
    startTrainingMock.mockResolvedValue({ id: 'job-1' });

    render(
      <TrainingWizard
        onComplete={vi.fn()}
        onCancel={() => {}}
        initialDatasetId="ds-draft"
      />,
    );

    const user = userEvent.setup();

    await screen.findByText(/Draft Dataset/);

    await user.click(screen.getByRole('button', { name: /Next/i }));

    expect(
      await screen.findByText(/must be validated/i),
    ).toBeTruthy();
    expect(startTrainingMock).not.toHaveBeenCalled();
  });

  it('allows training when dataset is validated', async () => {
    listDatasetsMock.mockResolvedValue({
      datasets: [{ id: 'ds-valid', name: 'Valid Dataset', validation_status: 'valid' }],
    });
    startTrainingMock.mockResolvedValue({ id: 'job-42' });
    const onComplete = vi.fn();

    render(
      <TrainingWizard
        onComplete={onComplete}
        onCancel={() => {}}
        initialDatasetId="ds-valid"
      />,
    );

    const user = userEvent.setup();

    await screen.findByText(/Valid Dataset/);

    await user.click(screen.getByRole('button', { name: /Next/i }));
    await user.click(screen.getByRole('button', { name: /Next/i }));
    await user.click(screen.getByRole('button', { name: /Start Training/i }));

    await waitFor(() => expect(startTrainingMock).toHaveBeenCalled());
    expect(onComplete).toHaveBeenCalledWith('job-42');
  });
});
