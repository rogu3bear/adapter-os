import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import userEvent from '@testing-library/user-event';
import { TrainingWizard } from '@/components/TrainingWizard';

const listDatasetsMock = vi.fn();
const startTrainingMock = vi.fn();

vi.mock('@/api/services', () => ({
  __esModule: true,
  apiClient: {
    listRepositories: vi.fn().mockResolvedValue([]),
    listTrainingTemplates: vi.fn().mockResolvedValue([]),
    listDatasets: (...args: unknown[]) => listDatasetsMock(...args),
    startTraining: (...args: unknown[]) => startTrainingMock(...args),
  },
  default: {
    listRepositories: vi.fn().mockResolvedValue([]),
    listTrainingTemplates: vi.fn().mockResolvedValue([]),
    listDatasets: (...args: unknown[]) => listDatasetsMock(...args),
    startTraining: (...args: unknown[]) => startTrainingMock(...args),
  },
}));

vi.mock('@/schemas', async () => {
  const actual = await vi.importActual<typeof import('@/schemas')>('@/schemas');
  return {
    ...actual,
    TrainingConfigSchema: {
      parseAsync: vi.fn(async (value: unknown) => value),
    },
  };
});

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
    info: vi.fn(),
    debug: vi.fn(),
  },
  toError: (error: unknown) => error,
}));

vi.mock('@/hooks/documents', () => ({
  useDocuments: () => ({ data: [], isLoading: false, error: null }),
}));

vi.mock('@/hooks/api/useCollectionsApi', () => ({
  useCollections: () => ({ data: [], isLoading: false, error: null }),
}));

vi.mock('@/hooks/training', () => ({
  useTrainingDataOrchestrator: () => ({
    orchestrate: vi.fn(),
  }),
}));

vi.mock('@/components/BreadcrumbNavigation', () => ({
  BreadcrumbNavigation: () => null,
}));

// Simplify Select to avoid Radix pointer-capture issues in JSDOM
vi.mock('@/components/ui/select', () => {
  const React = require('react');
  const Select = ({ value, onValueChange, children, ...props }: any) => (
    <select
      value={value ?? ''}
      onChange={(e) => onValueChange?.((e.target as HTMLSelectElement).value)}
      {...props}
    >
      {children}
    </select>
  );

  return {
    Select,
    SelectTrigger: ({ children }: any) => <>{children}</>,
    SelectContent: ({ children }: any) => <>{children}</>,
    SelectItem: ({ value, children, ...props }: any) => (
      <option value={value} {...props}>
        {children}
      </option>
    ),
    SelectValue: ({ placeholder }: any) => (
      <option value="" hidden>
        {placeholder}
      </option>
    ),
  };
});

describe('TrainingWizard dataset validation guard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('blocks training when selected dataset is not validated', async () => {
    listDatasetsMock.mockResolvedValue({
      datasets: [{
        id: 'ds-draft',
        name: 'Draft Dataset',
        validation_status: 'pending',
        hash_b3: 'hash123',
        source_type: 'jsonl',
        file_count: 10,
        total_size_bytes: 1024 * 1024,
        total_tokens: 1000,
        created_at: '2025-01-01T00:00:00Z',
        updated_at: '2025-01-01T00:00:00Z',
      }],
    });
    startTrainingMock.mockResolvedValue({ id: 'job-1' });

    render(
      <MemoryRouter>
        <TrainingWizard
          onComplete={vi.fn()}
          onCancel={() => {}}
          initialDatasetId="ds-draft"
        />
      </MemoryRouter>,
    );

    const user = userEvent.setup();

    await screen.findByRole('button', { name: /Next/i });

    await user.click(screen.getByRole('button', { name: /Next/i }));

    expect(
      (await screen.findAllByText(/must be validated/i)).length,
    ).toBeTruthy();
    expect(startTrainingMock).not.toHaveBeenCalled();
  });

  it('allows training when dataset is validated', async () => {
    listDatasetsMock.mockResolvedValue({
      datasets: [{
        id: 'ds-valid',
        name: 'Valid Dataset',
        validation_status: 'valid',
        hash_b3: 'hash456',
        source_type: 'jsonl',
        file_count: 20,
        total_size_bytes: 2 * 1024 * 1024,
        total_tokens: 5000,
        created_at: '2025-01-01T00:00:00Z',
        updated_at: '2025-01-01T00:00:00Z',
      }],
    });
    startTrainingMock.mockResolvedValue({ id: 'job-42' });
    const onComplete = vi.fn();

    render(
      <MemoryRouter>
        <TrainingWizard
          onComplete={onComplete}
          onCancel={() => {}}
        />
      </MemoryRouter>,
    );

    const user = userEvent.setup();

    await waitFor(() => expect(listDatasetsMock).toHaveBeenCalled());
    await user.selectOptions(screen.getByRole('combobox'), 'ds-valid');

    await user.click(screen.getByRole('button', { name: /Next/i }));
    await user.click(screen.getByRole('button', { name: /Next/i }));
    await user.click(screen.getByRole('button', { name: /Start Training/i }));

    await waitFor(() => expect(startTrainingMock).toHaveBeenCalled());
    expect(onComplete).toHaveBeenCalledWith('job-42');
  });
});
