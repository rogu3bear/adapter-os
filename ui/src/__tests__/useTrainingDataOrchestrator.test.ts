import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useTrainingDataOrchestrator } from '@/hooks/useTrainingDataOrchestrator';
import apiClient from '@/api/client';

vi.mock('@/api/client', () => ({
  __esModule: true,
  default: {
    uploadDocument: vi.fn(),
    processDocument: vi.fn(),
    createDatasetFromDocuments: vi.fn(),
  },
}));

const mockedApi = apiClient as unknown as {
  uploadDocument: ReturnType<typeof vi.fn>;
  processDocument: ReturnType<typeof vi.fn>;
  createDatasetFromDocuments: ReturnType<typeof vi.fn>;
};

describe('useTrainingDataOrchestrator', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('creates a dataset from a file upload via the canonical pipeline', async () => {
    const file = new File(['hello'], 'readme.md', { type: 'text/markdown' });

    mockedApi.uploadDocument.mockResolvedValue({ document_id: 'doc-1' });
    mockedApi.processDocument.mockResolvedValue(undefined);
    mockedApi.createDatasetFromDocuments.mockResolvedValue({
      dataset_id: 'ds-1',
      name: 'tenant/domain/purpose/r001',
      validation_status: 'valid',
    });

    const { result } = renderHook(() => useTrainingDataOrchestrator());

    let datasetId: string | undefined;
    await act(async () => {
      const output = await result.current.orchestrate({
        kind: 'file',
        file,
        name: 'tenant/domain/purpose/r001',
        description: 'Training dataset derived from readme.md',
      });
      datasetId = output.datasetId;
    });

    expect(datasetId).toBe('ds-1');
    expect(result.current.state.datasetId).toBe('ds-1');
    expect(result.current.state.status).toBe('done');

    expect(mockedApi.uploadDocument).toHaveBeenCalledWith({
      file,
      name: 'tenant/domain/purpose/r001',
      description: 'Training dataset derived from readme.md',
    });
    expect(mockedApi.processDocument).toHaveBeenCalledWith('doc-1');
    expect(mockedApi.createDatasetFromDocuments).toHaveBeenCalledWith({
      document_ids: ['doc-1'],
      documentId: 'doc-1',
      name: 'tenant/domain/purpose/r001',
      description: 'Training dataset derived from readme.md',
    });
  });

  it('surfaces errors and moves to error status', async () => {
    const file = new File(['hello'], 'readme.md', { type: 'text/markdown' });

    mockedApi.uploadDocument.mockResolvedValue({ document_id: 'doc-err' });
    mockedApi.processDocument.mockResolvedValue(undefined);
    mockedApi.createDatasetFromDocuments.mockRejectedValue(new Error('create failed'));

    const { result } = renderHook(() => useTrainingDataOrchestrator());

    await act(async () => {
      await expect(
        result.current.orchestrate({
          kind: 'file',
          file,
        })
      ).rejects.toThrow('create failed');
    });

    expect(result.current.state.status).toBe('error');
    expect(result.current.state.error).toBe('create failed');
  });
});

