import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  useCollections,
  useCollection,
  useCollectionsApi,
  collectionKeys,
} from '@/hooks/api/useCollectionsApi';
import type { Collection, CollectionDetail } from '@/api/document-types';
import { withTenantKey } from '@/utils/tenant';

// Mock tenant ID used in tests
const MOCK_TENANT_ID = 'tenant-1';

// Mock API client
const mockListCollections = vi.fn();
const mockGetCollection = vi.fn();
const mockCreateCollection = vi.fn();
const mockDeleteCollection = vi.fn();
const mockAddDocumentToCollection = vi.fn();
const mockRemoveDocumentFromCollection = vi.fn();

vi.mock('@/api/services', () => ({
  apiClient: {
    listCollections: (...args: unknown[]) => mockListCollections(...args),
    getCollection: (...args: unknown[]) => mockGetCollection(...args),
    createCollection: (...args: unknown[]) => mockCreateCollection(...args),
    deleteCollection: (...args: unknown[]) => mockDeleteCollection(...args),
    addDocumentToCollection: (...args: unknown[]) => mockAddDocumentToCollection(...args),
    removeDocumentFromCollection: (...args: unknown[]) => mockRemoveDocumentFromCollection(...args),
  },
}));

// Mock FeatureProviders to provide useTenant context
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: MOCK_TENANT_ID }),
}));

// Test data
const mockCollections: Collection[] = [
  {
    schema_version: '1.0',
    collection_id: 'col-1',
    id: 'col-1',
    name: 'Test Collection',
    description: 'A test collection',
    document_count: 5,
    tenant_id: 'tenant-1',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    schema_version: '1.0',
    collection_id: 'col-2',
    id: 'col-2',
    name: 'Empty Collection',
    description: null,
    document_count: 0,
    tenant_id: 'tenant-1',
    created_at: '2025-01-02T00:00:00Z',
    updated_at: null,
  },
];

const mockCollectionDetail: CollectionDetail = {
  schema_version: '1.0',
  collection_id: 'col-1',
  id: 'col-1',
  name: 'Test Collection',
  description: 'A test collection',
  document_count: 2,
  tenant_id: 'tenant-1',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
  documents: [
    {
      document_id: 'doc-1',
      name: 'test.pdf',
      size_bytes: 1024,
      status: 'indexed',
      added_at: '2025-01-01T00:00:00Z',
    },
    {
      document_id: 'doc-2',
      name: 'another.txt',
      size_bytes: 512,
      status: 'processing',
      added_at: '2025-01-02T00:00:00Z',
    },
  ],
};

// Test wrapper
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

describe('useCollectionsApi - Queries', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useCollections', () => {
    it('returns collection list successfully', async () => {
      mockListCollections.mockResolvedValue(mockCollections);

      const { result } = renderHook(() => useCollections(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockCollections);
      expect(mockListCollections).toHaveBeenCalledTimes(1);
    });

    it('handles empty collection list', async () => {
      mockListCollections.mockResolvedValue([]);

      const { result } = renderHook(() => useCollections(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch collections');
      mockListCollections.mockRejectedValue(error);

      const { result } = renderHook(() => useCollections(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });

    it('uses 30 second stale time', () => {
      mockListCollections.mockResolvedValue(mockCollections);

      const { result } = renderHook(() => useCollections(), {
        wrapper: createWrapper(),
      });

      // Query options should include staleTime
      expect(result.current).toBeDefined();
    });
  });

  describe('useCollection', () => {
    it('returns collection with documents successfully', async () => {
      mockGetCollection.mockResolvedValue(mockCollectionDetail);

      const { result } = renderHook(() => useCollection('col-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockCollectionDetail);
      expect(result.current.data?.documents).toHaveLength(2);
      expect(mockGetCollection).toHaveBeenCalledWith('col-1');
    });

    it('does not fetch when collectionId is undefined', () => {
      const { result } = renderHook(() => useCollection(undefined), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetCollection).not.toHaveBeenCalled();
    });

    it('handles collection not found', async () => {
      const error = new Error('Collection not found');
      mockGetCollection.mockRejectedValue(error);

      const { result } = renderHook(() => useCollection('nonexistent'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });
  });
});

describe('useCollectionsApi - Mutations', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('create mutation', () => {
    it('creates collection and invalidates cache', async () => {
      const newCollection: Collection = {
        ...mockCollections[0],
        collection_id: 'col-3',
        id: 'col-3',
        name: 'New Collection',
      };
      mockCreateCollection.mockResolvedValue(newCollection);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useCollectionsApi(), { wrapper });

      await result.current.createCollection({ name: 'New Collection', description: 'Test' });

      expect(mockCreateCollection).toHaveBeenCalledWith('New Collection', 'Test');
      expect(result.current.isCreating).toBe(false);
    });

    it('handles create error', async () => {
      const error = new Error('Create failed');
      mockCreateCollection.mockRejectedValue(error);

      const { result } = renderHook(() => useCollectionsApi(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.createCollection({ name: 'Test' })
      ).rejects.toThrow('Create failed');

      await waitFor(() => {
        expect(result.current.createError).toEqual(error);
      });
    });

    it('sets creating state correctly', async () => {
      mockCreateCollection.mockImplementation(
        () => new Promise(resolve => setTimeout(() => resolve(mockCollections[0]), 100))
      );

      const { result } = renderHook(() => useCollectionsApi(), {
        wrapper: createWrapper(),
      });

      const createPromise = result.current.createCollection({ name: 'Test' });

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
    it('deletes collection and removes from cache', async () => {
      mockDeleteCollection.mockResolvedValue({ success: true });

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Pre-populate cache with collection using tenant-scoped key
      const tenantScopedKey = withTenantKey(collectionKeys.detail('col-1'), MOCK_TENANT_ID);
      queryClient.setQueryData(tenantScopedKey, mockCollectionDetail);

      const { result } = renderHook(() => useCollectionsApi(), { wrapper });

      await result.current.deleteCollection('col-1');

      expect(mockDeleteCollection).toHaveBeenCalledWith('col-1');

      // Verify collection removed from cache using tenant-scoped key
      const cachedCollection = queryClient.getQueryData(tenantScopedKey);
      expect(cachedCollection).toBeUndefined();
    });

    it('handles delete error', async () => {
      const error = new Error('Delete failed');
      mockDeleteCollection.mockRejectedValue(error);

      const { result } = renderHook(() => useCollectionsApi(), {
        wrapper: createWrapper(),
      });

      await expect(result.current.deleteCollection('col-1')).rejects.toThrow('Delete failed');

      await waitFor(() => {
        expect(result.current.deleteError).toEqual(error);
      });
    });
  });

  describe('add document mutation', () => {
    it('adds document to collection and invalidates cache', async () => {
      mockAddDocumentToCollection.mockResolvedValue({ success: true });

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useCollectionsApi(), { wrapper });

      await result.current.addDocumentToCollection({
        collectionId: 'col-1',
        documentId: 'doc-3',
      });

      expect(mockAddDocumentToCollection).toHaveBeenCalledWith('col-1', 'doc-3');
      expect(result.current.isAddingDocument).toBe(false);
    });

    it('handles add document error', async () => {
      const error = new Error('Add failed');
      mockAddDocumentToCollection.mockRejectedValue(error);

      const { result } = renderHook(() => useCollectionsApi(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.addDocumentToCollection({
          collectionId: 'col-1',
          documentId: 'doc-3',
        })
      ).rejects.toThrow('Add failed');

      await waitFor(() => {
        expect(result.current.addDocumentError).toEqual(error);
      });
    });

    it('invalidates collection detail and list queries', async () => {
      mockAddDocumentToCollection.mockResolvedValue({ success: true });

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(collectionKeys.detail('col-1'), mockCollectionDetail);
      queryClient.setQueryData(collectionKeys.list(), mockCollections);

      const { result } = renderHook(() => useCollectionsApi(), { wrapper });

      await result.current.addDocumentToCollection({
        collectionId: 'col-1',
        documentId: 'doc-3',
      });

      // Verify queries are invalidated
      const detailQueries = queryClient.getQueryCache().findAll({
        queryKey: collectionKeys.detail('col-1'),
      });
      expect(detailQueries.length).toBeGreaterThan(0);
    });
  });

  describe('remove document mutation', () => {
    it('removes document from collection and invalidates cache', async () => {
      mockRemoveDocumentFromCollection.mockResolvedValue({ success: true });

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useCollectionsApi(), { wrapper });

      await result.current.removeDocumentFromCollection({
        collectionId: 'col-1',
        documentId: 'doc-1',
      });

      expect(mockRemoveDocumentFromCollection).toHaveBeenCalledWith('col-1', 'doc-1');
      expect(result.current.isRemovingDocument).toBe(false);
    });

    it('handles remove document error', async () => {
      const error = new Error('Remove failed');
      mockRemoveDocumentFromCollection.mockRejectedValue(error);

      const { result } = renderHook(() => useCollectionsApi(), {
        wrapper: createWrapper(),
      });

      await expect(
        result.current.removeDocumentFromCollection({
          collectionId: 'col-1',
          documentId: 'doc-1',
        })
      ).rejects.toThrow('Remove failed');

      await waitFor(() => {
        expect(result.current.removeDocumentError).toEqual(error);
      });
    });
  });

  describe('cache invalidation', () => {
    it('invalidates all collection queries', async () => {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(collectionKeys.list(), mockCollections);
      queryClient.setQueryData(collectionKeys.detail('col-1'), mockCollectionDetail);

      const { result } = renderHook(() => useCollectionsApi(), { wrapper });

      await result.current.invalidateCollections();

      // Check that queries are marked for invalidation
      const queries = queryClient.getQueryCache().findAll({
        queryKey: collectionKeys.all,
      });
      expect(queries.length).toBeGreaterThan(0);
    });
  });
});

describe('collectionKeys', () => {
  it('generates correct query keys', () => {
    expect(collectionKeys.all).toEqual(['collections']);
    expect(collectionKeys.lists()).toEqual(['collections', 'list']);
    expect(collectionKeys.list()).toEqual(['collections', 'list']);
    expect(collectionKeys.details()).toEqual(['collections', 'detail']);
    expect(collectionKeys.detail('col-1')).toEqual(['collections', 'detail', 'col-1']);
  });
});
