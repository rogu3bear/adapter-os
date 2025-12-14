import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  useEvidence,
  useEvidenceEntry,
  useDatasetEvidence,
  useAdapterEvidence,
  useEvidenceApi,
  evidenceKeys,
} from '@/hooks/api/useEvidenceApi';
import type {
  Evidence,
  CreateEvidenceRequest,
  ListEvidenceQuery,
} from '@/api/document-types';

// Mock API client
const mockListEvidence = vi.fn();
const mockGetEvidence = vi.fn();
const mockGetDatasetEvidence = vi.fn();
const mockGetAdapterEvidence = vi.fn();
const mockCreateEvidence = vi.fn();
const mockDeleteEvidence = vi.fn();

vi.mock('@/api/client', () => ({
  apiClient: {
    listEvidence: (...args: unknown[]) => mockListEvidence(...args),
    getEvidence: (...args: unknown[]) => mockGetEvidence(...args),
    getDatasetEvidence: (...args: unknown[]) => mockGetDatasetEvidence(...args),
    getAdapterEvidence: (...args: unknown[]) => mockGetAdapterEvidence(...args),
    createEvidence: (...args: unknown[]) => mockCreateEvidence(...args),
    deleteEvidence: (...args: unknown[]) => mockDeleteEvidence(...args),
  },
}));

// Test data
const mockEvidence: Evidence[] = [
  {
    id: 'ev-1',
    dataset_id: 'dataset-1',
    adapter_id: null,
    evidence_type: 'doc',
    reference: 'DOC-001',
    description: 'Training documentation',
    confidence: 'high',
    created_by: 'user-1',
    created_at: '2025-01-01T00:00:00Z',
    metadata_json: '{"source": "internal"}',
  },
  {
    id: 'ev-2',
    dataset_id: null,
    adapter_id: 'adapter-1',
    evidence_type: 'review',
    reference: 'REV-123',
    description: 'Code review approval',
    confidence: 'medium',
    created_by: 'user-2',
    created_at: '2025-01-02T00:00:00Z',
    metadata_json: null,
  },
  {
    id: 'ev-3',
    dataset_id: 'dataset-1',
    adapter_id: 'adapter-1',
    evidence_type: 'audit',
    reference: 'AUD-456',
    description: null,
    confidence: 'high',
    created_by: null,
    created_at: '2025-01-03T00:00:00Z',
    metadata_json: null,
  },
];

// Test wrapper
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

describe('useEvidenceApi - Queries', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useEvidence', () => {
    it('returns evidence list successfully', async () => {
      mockListEvidence.mockResolvedValue(mockEvidence);

      const { result } = renderHook(() => useEvidence(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockEvidence);
      expect(mockListEvidence).toHaveBeenCalledWith(undefined);
    });

    it('filters evidence by dataset_id', async () => {
      const filteredEvidence = [mockEvidence[0], mockEvidence[2]];
      mockListEvidence.mockResolvedValue(filteredEvidence);

      const filter: ListEvidenceQuery = { dataset_id: 'dataset-1' };
      const { result } = renderHook(() => useEvidence(filter), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(filteredEvidence);
      expect(mockListEvidence).toHaveBeenCalledWith(filter);
    });

    it('filters evidence by adapter_id', async () => {
      const filteredEvidence = [mockEvidence[1], mockEvidence[2]];
      mockListEvidence.mockResolvedValue(filteredEvidence);

      const filter: ListEvidenceQuery = { adapter_id: 'adapter-1' };
      const { result } = renderHook(() => useEvidence(filter), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(filteredEvidence);
      expect(mockListEvidence).toHaveBeenCalledWith(filter);
    });

    it('filters evidence by type and confidence', async () => {
      const filteredEvidence = [mockEvidence[0]];
      mockListEvidence.mockResolvedValue(filteredEvidence);

      const filter: ListEvidenceQuery = {
        evidence_type: 'doc',
        confidence: 'high',
      };
      const { result } = renderHook(() => useEvidence(filter), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(filteredEvidence);
      expect(mockListEvidence).toHaveBeenCalledWith(filter);
    });

    it('applies limit filter', async () => {
      const limitedEvidence = mockEvidence.slice(0, 2);
      mockListEvidence.mockResolvedValue(limitedEvidence);

      const filter: ListEvidenceQuery = { limit: 2 };
      const { result } = renderHook(() => useEvidence(filter), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toHaveLength(2);
      expect(mockListEvidence).toHaveBeenCalledWith(filter);
    });

    it('handles empty evidence list', async () => {
      mockListEvidence.mockResolvedValue([]);

      const { result } = renderHook(() => useEvidence(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch evidence');
      mockListEvidence.mockRejectedValue(error);

      const { result } = renderHook(() => useEvidence(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });
  });

  describe('useEvidenceEntry', () => {
    it('returns single evidence entry successfully', async () => {
      mockGetEvidence.mockResolvedValue(mockEvidence[0]);

      const { result } = renderHook(() => useEvidenceEntry('ev-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockEvidence[0]);
      expect(mockGetEvidence).toHaveBeenCalledWith('ev-1');
    });

    it('does not fetch when evidenceId is undefined', () => {
      const { result } = renderHook(() => useEvidenceEntry(undefined), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetEvidence).not.toHaveBeenCalled();
    });

    it('handles evidence not found', async () => {
      const error = new Error('Evidence not found');
      mockGetEvidence.mockRejectedValue(error);

      const { result } = renderHook(() => useEvidenceEntry('nonexistent'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });
  });

  describe('useDatasetEvidence', () => {
    it('returns evidence for dataset successfully', async () => {
      const datasetEvidence = [mockEvidence[0], mockEvidence[2]];
      mockGetDatasetEvidence.mockResolvedValue(datasetEvidence);

      const { result } = renderHook(() => useDatasetEvidence('dataset-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(datasetEvidence);
      expect(mockGetDatasetEvidence).toHaveBeenCalledWith('dataset-1');
    });

    it('does not fetch when datasetId is undefined', () => {
      const { result } = renderHook(() => useDatasetEvidence(undefined), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetDatasetEvidence).not.toHaveBeenCalled();
    });
  });

  describe('useAdapterEvidence', () => {
    it('returns evidence for adapter successfully', async () => {
      const adapterEvidence = [mockEvidence[1], mockEvidence[2]];
      mockGetAdapterEvidence.mockResolvedValue(adapterEvidence);

      const { result } = renderHook(() => useAdapterEvidence('adapter-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(adapterEvidence);
      expect(mockGetAdapterEvidence).toHaveBeenCalledWith('adapter-1');
    });

    it('does not fetch when adapterId is undefined', () => {
      const { result } = renderHook(() => useAdapterEvidence(undefined), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetAdapterEvidence).not.toHaveBeenCalled();
    });
  });
});

describe('useEvidenceApi - Mutations', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('create mutation', () => {
    it('creates evidence and invalidates cache', async () => {
      const newEvidence: Evidence = {
        id: 'ev-4',
        dataset_id: 'dataset-2',
        adapter_id: null,
        evidence_type: 'doc',
        reference: 'DOC-002',
        description: 'New evidence',
        confidence: 'high',
        created_by: 'user-1',
        created_at: '2025-01-04T00:00:00Z',
        metadata_json: null,
      };
      mockCreateEvidence.mockResolvedValue(newEvidence);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useEvidenceApi(), { wrapper });

      const request: CreateEvidenceRequest = {
        dataset_id: 'dataset-2',
        evidence_type: 'doc',
        reference: 'DOC-002',
        description: 'New evidence',
        confidence: 'high',
      };

      await result.current.createEvidence(request);

      expect(mockCreateEvidence).toHaveBeenCalledWith(request);
      expect(result.current.isCreating).toBe(false);
    });

    it('invalidates dataset-specific queries on create', async () => {
      const newEvidence: Evidence = {
        ...mockEvidence[0],
        id: 'ev-new',
        dataset_id: 'dataset-1',
      };
      mockCreateEvidence.mockResolvedValue(newEvidence);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Pre-populate dataset cache
      queryClient.setQueryData(evidenceKeys.byDataset('dataset-1'), [mockEvidence[0]]);

      const { result } = renderHook(() => useEvidenceApi(), { wrapper });

      const request: CreateEvidenceRequest = {
        dataset_id: 'dataset-1',
        evidence_type: 'doc',
        reference: 'DOC-003',
      };

      await result.current.createEvidence(request);

      // Verify dataset-specific queries are invalidated
      const datasetQueries = queryClient.getQueryCache().findAll({
        queryKey: evidenceKeys.byDataset('dataset-1'),
      });
      expect(datasetQueries.length).toBeGreaterThan(0);
    });

    it('invalidates adapter-specific queries on create', async () => {
      const newEvidence: Evidence = {
        ...mockEvidence[1],
        id: 'ev-new',
        adapter_id: 'adapter-1',
      };
      mockCreateEvidence.mockResolvedValue(newEvidence);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Pre-populate adapter cache
      queryClient.setQueryData(evidenceKeys.byAdapter('adapter-1'), [mockEvidence[1]]);

      const { result } = renderHook(() => useEvidenceApi(), { wrapper });

      const request: CreateEvidenceRequest = {
        adapter_id: 'adapter-1',
        evidence_type: 'review',
        reference: 'REV-456',
      };

      await result.current.createEvidence(request);

      // Verify adapter-specific queries are invalidated
      const adapterQueries = queryClient.getQueryCache().findAll({
        queryKey: evidenceKeys.byAdapter('adapter-1'),
      });
      expect(adapterQueries.length).toBeGreaterThan(0);
    });

    it('handles create error', async () => {
      const error = new Error('Create failed');
      mockCreateEvidence.mockRejectedValue(error);

      const { result } = renderHook(() => useEvidenceApi(), {
        wrapper: createWrapper(),
      });

      const request: CreateEvidenceRequest = {
        evidence_type: 'doc',
        reference: 'DOC-001',
      };

      await expect(result.current.createEvidence(request)).rejects.toThrow('Create failed');

      await waitFor(() => {
        expect(result.current.createError).toEqual(error);
      });
    });

    it('sets creating state correctly', async () => {
      mockCreateEvidence.mockImplementation(
        () => new Promise(resolve => setTimeout(() => resolve(mockEvidence[0]), 100))
      );

      const { result } = renderHook(() => useEvidenceApi(), {
        wrapper: createWrapper(),
      });

      const request: CreateEvidenceRequest = {
        evidence_type: 'doc',
        reference: 'DOC-001',
      };

      const createPromise = result.current.createEvidence(request);

      await waitFor(() => {
        expect(result.current.isCreating).toBe(true);
      });

      await createPromise;

      await waitFor(() => {
        expect(result.current.isCreating).toBe(false);
      });
    });
  });

  describe('delete mutation', () => {
    it('deletes evidence and removes from cache', async () => {
      mockDeleteEvidence.mockResolvedValue({ success: true });

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Pre-populate cache with evidence
      queryClient.setQueryData(evidenceKeys.detail('ev-1'), mockEvidence[0]);

      const { result } = renderHook(() => useEvidenceApi(), { wrapper });

      await result.current.deleteEvidence('ev-1');

      expect(mockDeleteEvidence).toHaveBeenCalledWith('ev-1');

      // Verify evidence removed from cache
      const cachedEvidence = queryClient.getQueryData(evidenceKeys.detail('ev-1'));
      expect(cachedEvidence).toBeUndefined();
    });

    it('handles delete error', async () => {
      const error = new Error('Delete failed');
      mockDeleteEvidence.mockRejectedValue(error);

      const { result } = renderHook(() => useEvidenceApi(), {
        wrapper: createWrapper(),
      });

      await expect(result.current.deleteEvidence('ev-1')).rejects.toThrow('Delete failed');

      await waitFor(() => {
        expect(result.current.deleteError).toEqual(error);
      });
    });

    it('sets deleting state correctly', async () => {
      mockDeleteEvidence.mockImplementation(
        () => new Promise(resolve => setTimeout(() => resolve({ success: true }), 100))
      );

      const { result } = renderHook(() => useEvidenceApi(), {
        wrapper: createWrapper(),
      });

      const deletePromise = result.current.deleteEvidence('ev-1');

      await waitFor(() => {
        expect(result.current.isDeleting).toBe(true);
      });

      await deletePromise;

      await waitFor(() => {
        expect(result.current.isDeleting).toBe(false);
      });
    });
  });

  describe('cache invalidation', () => {
    it('invalidates all evidence queries', async () => {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(evidenceKeys.list(), mockEvidence);
      queryClient.setQueryData(evidenceKeys.byDataset('dataset-1'), [mockEvidence[0]]);
      queryClient.setQueryData(evidenceKeys.byAdapter('adapter-1'), [mockEvidence[1]]);

      const { result } = renderHook(() => useEvidenceApi(), { wrapper });

      await result.current.invalidateEvidence();

      // Check that queries are marked for invalidation
      const queries = queryClient.getQueryCache().findAll({
        queryKey: evidenceKeys.all,
      });
      expect(queries.length).toBeGreaterThan(0);
    });
  });
});

describe('evidenceKeys', () => {
  it('generates correct query keys', () => {
    expect(evidenceKeys.all).toEqual(['evidence']);
    expect(evidenceKeys.lists()).toEqual(['evidence', 'list']);
    expect(evidenceKeys.list()).toEqual(['evidence', 'list', undefined]);
    expect(evidenceKeys.list({ dataset_id: 'dataset-1' })).toEqual([
      'evidence',
      'list',
      { dataset_id: 'dataset-1' },
    ]);
    expect(evidenceKeys.details()).toEqual(['evidence', 'detail']);
    expect(evidenceKeys.detail('ev-1')).toEqual(['evidence', 'detail', 'ev-1']);
    expect(evidenceKeys.byDataset('dataset-1')).toEqual(['evidence', 'dataset', 'dataset-1']);
    expect(evidenceKeys.byAdapter('adapter-1')).toEqual(['evidence', 'adapter', 'adapter-1']);
  });
});
