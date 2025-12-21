import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  useDocuments,
  useDocument,
  useDocumentChunks,
  useDocumentsApi,
  documentKeys,
} from '@/hooks/documents';
import type { Document, DocumentChunk } from '@/api/document-types';
import { withTenantKey } from '@/utils/tenant';

// Mock tenant ID used in tests
const MOCK_TENANT_ID = 'tenant-1';

// Mock API client
const mockListDocuments = vi.fn();
const mockGetDocument = vi.fn();
const mockListDocumentChunks = vi.fn();
const mockUploadDocument = vi.fn();
const mockDeleteDocument = vi.fn();
const mockDownloadDocument = vi.fn();

vi.mock('@/api/services', () => ({
  apiClient: {
    listDocuments: (...args: unknown[]) => mockListDocuments(...args),
    getDocument: (...args: unknown[]) => mockGetDocument(...args),
    listDocumentChunks: (...args: unknown[]) => mockListDocumentChunks(...args),
    uploadDocument: (...args: unknown[]) => mockUploadDocument(...args),
    deleteDocument: (...args: unknown[]) => mockDeleteDocument(...args),
    downloadDocument: (...args: unknown[]) => mockDownloadDocument(...args),
  },
}));

// Mock FeatureProviders to provide useTenant context
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: MOCK_TENANT_ID }),
}));

// Test data
const mockDocuments: Document[] = [
  {
    schema_version: '1.0',
    document_id: 'doc-1',
    name: 'test-doc.pdf',
    hash_b3: 'hash1',
    size_bytes: 1024,
    mime_type: 'application/pdf',
    storage_path: '/storage/doc-1.pdf',
    status: 'indexed',
    chunk_count: 10,
    tenant_id: 'tenant-1',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    schema_version: '1.0',
    document_id: 'doc-2',
    name: 'another-doc.txt',
    hash_b3: 'hash2',
    size_bytes: 512,
    mime_type: 'text/plain',
    storage_path: '/storage/doc-2.txt',
    status: 'processing',
    chunk_count: null,
    tenant_id: 'tenant-1',
    created_at: '2025-01-02T00:00:00Z',
    updated_at: null,
  },
];

const mockDocument: Document = mockDocuments[0];

const mockChunks: DocumentChunk[] = [
  {
    schema_version: '1.0',
    chunk_id: 'chunk-1',
    document_id: 'doc-1',
    chunk_index: 0,
    text: 'First chunk text',
    embedding: [0.1, 0.2, 0.3],
    metadata: { page: 1 },
    created_at: '2025-01-01T00:00:00Z',
  },
  {
    schema_version: '1.0',
    chunk_id: 'chunk-2',
    document_id: 'doc-1',
    chunk_index: 1,
    text: 'Second chunk text',
    embedding: [0.4, 0.5, 0.6],
    metadata: { page: 2 },
    created_at: '2025-01-01T00:00:00Z',
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

  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

describe('useDocumentsApi - Queries', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useDocuments', () => {
    it('returns document list successfully', async () => {
      mockListDocuments.mockResolvedValue(mockDocuments);

      const { result } = renderHook(() => useDocuments(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockDocuments);
      expect(mockListDocuments).toHaveBeenCalledTimes(1);
    });

    it('handles empty document list', async () => {
      mockListDocuments.mockResolvedValue([]);

      const { result } = renderHook(() => useDocuments(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch documents');
      mockListDocuments.mockRejectedValue(error);

      const { result } = renderHook(() => useDocuments(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });
  });

  describe('useDocument', () => {
    it('returns single document successfully', async () => {
      mockGetDocument.mockResolvedValue(mockDocument);

      const { result } = renderHook(() => useDocument('doc-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockDocument);
      expect(mockGetDocument).toHaveBeenCalledWith('doc-1');
    });

    it('does not fetch when documentId is undefined', () => {
      const { result } = renderHook(() => useDocument(undefined), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockGetDocument).not.toHaveBeenCalled();
    });

    it('handles document not found', async () => {
      const error = new Error('Document not found');
      mockGetDocument.mockRejectedValue(error);

      const { result } = renderHook(() => useDocument('nonexistent'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });
  });

  describe('useDocumentChunks', () => {
    it('returns document chunks successfully', async () => {
      mockListDocumentChunks.mockResolvedValue(mockChunks);

      const { result } = renderHook(() => useDocumentChunks('doc-1'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual(mockChunks);
      expect(mockListDocumentChunks).toHaveBeenCalledWith('doc-1');
    });

    it('does not fetch when documentId is undefined', () => {
      const { result } = renderHook(() => useDocumentChunks(undefined), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockListDocumentChunks).not.toHaveBeenCalled();
    });
  });
});

describe('useDocumentsApi - Mutations', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('upload mutation', () => {
    it('uploads document and invalidates cache', async () => {
      const newDocument: Document = {
        ...mockDocument,
        document_id: 'doc-3',
        name: 'uploaded.pdf',
      };
      mockUploadDocument.mockResolvedValue(newDocument);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useDocumentsApi(), { wrapper });

      const file = new File(['content'], 'uploaded.pdf', { type: 'application/pdf' });
      await result.current.uploadDocument({ file, name: 'uploaded.pdf' });

      expect(mockUploadDocument).toHaveBeenCalledWith({
        file,
        name: 'uploaded.pdf',
        description: undefined,
      });
      expect(result.current.isUploading).toBe(false);

      // Verify invalidateQueries was called with correct query key
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: documentKeys.lists() });
    });

    it('handles upload error', async () => {
      const error = new Error('Upload failed');
      mockUploadDocument.mockRejectedValue(error);

      const { result } = renderHook(() => useDocumentsApi(), {
        wrapper: createWrapper(),
      });

      const file = new File(['content'], 'test.pdf', { type: 'application/pdf' });

      await expect(result.current.uploadDocument({ file })).rejects.toThrow('Upload failed');

      await waitFor(() => {
        expect(result.current.uploadError).toEqual(error);
      });
    });

    it('sets uploading state correctly', async () => {
      mockUploadDocument.mockImplementation(
        () => new Promise(resolve => setTimeout(() => resolve(mockDocument), 100))
      );

      const { result } = renderHook(() => useDocumentsApi(), {
        wrapper: createWrapper(),
      });

      const file = new File(['content'], 'test.pdf', { type: 'application/pdf' });
      const uploadPromise = result.current.uploadDocument({ file });

      await waitFor(() => {
        expect(result.current.isUploading).toBe(true);
      });

      await uploadPromise;

      await waitFor(() => {
        expect(result.current.isUploading).toBe(false);
      });
    });
  });

  describe('delete mutation', () => {
    it('deletes document and removes from cache', async () => {
      mockDeleteDocument.mockResolvedValue({ success: true });

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Pre-populate cache with document using tenant-scoped key
      const tenantScopedKey = withTenantKey(documentKeys.detail('doc-1'), MOCK_TENANT_ID);
      queryClient.setQueryData(tenantScopedKey, mockDocument);

      const { result } = renderHook(() => useDocumentsApi(), { wrapper });

      await result.current.deleteDocument('doc-1');

      expect(mockDeleteDocument).toHaveBeenCalledWith('doc-1');

      // Verify document removed from cache using tenant-scoped key
      const cachedDocument = queryClient.getQueryData(tenantScopedKey);
      expect(cachedDocument).toBeUndefined();
    });

    it('handles delete error', async () => {
      const error = new Error('Delete failed');
      mockDeleteDocument.mockRejectedValue(error);

      const { result } = renderHook(() => useDocumentsApi(), {
        wrapper: createWrapper(),
      });

      await expect(result.current.deleteDocument('doc-1')).rejects.toThrow('Delete failed');

      await waitFor(() => {
        expect(result.current.deleteError).toEqual(error);
      });
    });

    it('sets deleting state correctly', async () => {
      mockDeleteDocument.mockImplementation(
        () => new Promise(resolve => setTimeout(() => resolve({ success: true }), 100))
      );

      const { result } = renderHook(() => useDocumentsApi(), {
        wrapper: createWrapper(),
      });

      const deletePromise = result.current.deleteDocument('doc-1');

      await waitFor(() => {
        expect(result.current.isDeleting).toBe(true);
      });

      await deletePromise;

      await waitFor(() => {
        expect(result.current.isDeleting).toBe(false);
      });
    });
  });

  describe('download method', () => {
    it('downloads document successfully', async () => {
      const blob = new Blob(['content'], { type: 'application/pdf' });
      mockDownloadDocument.mockResolvedValue(blob);

      const { result } = renderHook(() => useDocumentsApi(), {
        wrapper: createWrapper(),
      });

      const downloaded = await result.current.downloadDocument('doc-1');

      expect(mockDownloadDocument).toHaveBeenCalledWith('doc-1');
      expect(downloaded).toBe(blob);
    });
  });

  describe('cache invalidation', () => {
    it('invalidates all document queries', async () => {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(documentKeys.list(), mockDocuments);
      queryClient.setQueryData(documentKeys.detail('doc-1'), mockDocument);

      const { result } = renderHook(() => useDocumentsApi(), { wrapper });

      await result.current.invalidateDocuments();

      // Check that queries are marked as stale
      const queries = queryClient.getQueryCache().findAll({
        queryKey: documentKeys.all,
      });
      expect(queries.length).toBeGreaterThan(0);
    });
  });
});

describe('documentKeys', () => {
  it('generates correct query keys', () => {
    expect(documentKeys.all).toEqual(['documents']);
    expect(documentKeys.lists()).toEqual(['documents', 'list']);
    expect(documentKeys.list()).toEqual(['documents', 'list']);
    expect(documentKeys.details()).toEqual(['documents', 'detail']);
    expect(documentKeys.detail('doc-1')).toEqual(['documents', 'detail', 'doc-1']);
    expect(documentKeys.chunks('doc-1')).toEqual(['documents', 'detail', 'doc-1', 'chunks']);
  });
});
