import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { resolveDatasetPrefill, StartTrainingForm } from './StartTrainingForm';
import type { Dataset, DatasetVersionSelection } from '@/api/training-types';
import apiClient from '@/api/client';
import { useTenant } from '@/providers/FeatureProviders';

vi.mock('@/api/client', () => {
  const startTraining = vi.fn();
  return {
    __esModule: true,
    default: {
      listTrainingTemplates: vi.fn(),
      listDatasets: vi.fn(),
      listModels: vi.fn(),
      getBaseModelStatus: vi.fn(),
      loadBaseModel: vi.fn(),
      startTraining,
    },
  };
});

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: vi.fn(),
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

const datasets: Dataset[] = [
  { id: 'ds1', name: 'One' } as Dataset,
  { id: 'ds2', name: 'Two' } as Dataset,
];

describe('StartTrainingForm helpers', () => {
  it('returns matching dataset id when present', () => {
    expect(resolveDatasetPrefill(datasets, 'ds2')).toBe('ds2');
  });

  it('returns undefined when dataset not found', () => {
    expect(resolveDatasetPrefill(datasets, 'missing')).toBeUndefined();
  });

  it('returns undefined when no id provided', () => {
    expect(resolveDatasetPrefill(datasets, undefined)).toBeUndefined();
  });
});

describe('StartTrainingForm dataset version handling', () => {
  const startTrainingMock = apiClient.startTraining as unknown as vi.Mock;
  const listDatasetsMock = apiClient.listDatasets as unknown as vi.Mock;
  const listTemplatesMock = apiClient.listTrainingTemplates as unknown as vi.Mock;
  const listModelsMock = apiClient.listModels as unknown as vi.Mock;
  const getBaseModelStatusMock = apiClient.getBaseModelStatus as unknown as vi.Mock;
  const mockUseTenant = useTenant as unknown as vi.Mock;

  const baseDataset = {
    file_count: 1,
    total_tokens: 1000,
    hash_b3: 'hash',
    source_type: 'uploaded_files',
    total_size_bytes: 10,
    validation_status: 'valid',
    trust_state: 'allowed',
    created_at: 'now',
    updated_at: 'now',
  };

  beforeEach(() => {
    listTemplatesMock.mockResolvedValue([]);
    listModelsMock.mockResolvedValue([]);
    getBaseModelStatusMock.mockResolvedValue({ status: 'ready', model_id: 'model-1' });
    mockUseTenant.mockReturnValue({
      selectedTenant: 'tenant-1',
      tenants: [{ schema_version: 'v1', id: 'tenant-1', name: 'Tenant', status: 'high_assurance' }],
      setSelectedTenant: vi.fn(),
      isLoading: false,
      refreshTenants: vi.fn(),
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('sends dataset_version_ids with weight 1 and omits data_spec_hash', async () => {
    const datasetWithVersion: Dataset = {
      ...baseDataset,
      id: 'ds-1',
      name: 'Dataset One',
      dataset_version_id: 'dv-1',
    } as Dataset;

    listDatasetsMock.mockResolvedValue({ datasets: [datasetWithVersion] });
    startTrainingMock.mockResolvedValue({ id: 'job-123' });

    render(
      <StartTrainingForm
        onSuccess={vi.fn()}
        onCancel={() => {}}
      />
    );

    await waitFor(() => expect(listDatasetsMock).toHaveBeenCalled());

    const adapterInput = screen.getByPlaceholderText('organization/domain/purpose/r001');
    await userEvent.clear(adapterInput);
    await userEvent.type(adapterInput, 'tenant/domain/task/r001');

    const submit = screen.getByRole('button', { name: /start training/i });
    expect(submit).toBeEnabled();
    await userEvent.click(submit);

    await waitFor(() => expect(startTrainingMock).toHaveBeenCalled());
    const payload = startTrainingMock.mock.calls[0][0] as {
      dataset_version_ids?: DatasetVersionSelection[];
      data_spec_hash?: string;
    };

    expect(payload.dataset_version_ids).toEqual([
      { dataset_version_id: 'dv-1', weight: 1 },
    ]);
    expect('data_spec_hash' in payload).toBe(false);
  });

  it('disables submit and shows inline error when dataset version is missing', async () => {
    const datasetWithoutVersion: Dataset = {
      ...baseDataset,
      id: 'ds-2',
      name: 'Dataset Two',
    } as Dataset;

    listDatasetsMock.mockResolvedValue({ datasets: [datasetWithoutVersion] });
    startTrainingMock.mockResolvedValue({ id: 'job-999' });

    render(
      <StartTrainingForm
        onSuccess={vi.fn()}
        onCancel={() => {}}
      />
    );

    await waitFor(() => expect(listDatasetsMock).toHaveBeenCalled());

    const adapterInput = screen.getByPlaceholderText('organization/domain/purpose/r001');
    await userEvent.clear(adapterInput);
    await userEvent.type(adapterInput, 'tenant/domain/task/r001');

    // Click the "Data" tab to see the dataset version error
    const dataTab = screen.getByRole('tab', { name: /data/i });
    await userEvent.click(dataTab);

    await waitFor(() => {
      expect(
        screen.getByText('This dataset has no version bound. Please create a dataset version before training.'),
      ).toBeInTheDocument();
    });

    const submit = screen.getByRole('button', { name: /start training/i });
    expect(submit).toBeDisabled();
    expect(startTrainingMock).not.toHaveBeenCalled();
  });
});

