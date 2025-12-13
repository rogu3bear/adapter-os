/**
 * Integration tests for ApiClient.requestList method
 *
 * Verifies that requestList correctly extracts arrays from various
 * response formats returned by the backend.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Need to unmock the client to test the real implementation
vi.unmock('@/api/client');

describe('ApiClient.requestList', () => {
  let apiClient: typeof import('@/api/client').apiClient;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    // Import the actual module's singleton
    const clientModule = await vi.importActual<typeof import('@/api/client')>('@/api/client');
    apiClient = clientModule.apiClient;
    fetchMock = vi.fn();
    global.fetch = fetchMock;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const mockFetchResponse = (data: unknown, status = 200) => {
    const jsonString = JSON.stringify(data);
    fetchMock.mockResolvedValueOnce({
      ok: status >= 200 && status < 300,
      status,
      statusText: status === 200 ? 'OK' : 'Error',
      json: () => Promise.resolve(data),
      text: () => Promise.resolve(jsonString),
      headers: new Headers(),
    });
  };

  describe('handles PaginatedResponse format', () => {
    it('extracts data array from PaginatedResponse', async () => {
      const paginatedResponse = {
        schema_version: '1.0',
        data: [{ id: '1', name: 'Item 1' }, { id: '2', name: 'Item 2' }],
        total: 2,
        page: 1,
        limit: 20,
        pages: 1,
      };
      mockFetchResponse(paginatedResponse);

      const result = await apiClient.requestList<{ id: string; name: string }>('/v1/test');

      expect(result).toEqual([
        { id: '1', name: 'Item 1' },
        { id: '2', name: 'Item 2' },
      ]);
    });

    it('handles empty PaginatedResponse', async () => {
      const paginatedResponse = {
        schema_version: '1.0',
        data: [],
        total: 0,
        page: 1,
        limit: 20,
        pages: 0,
      };
      mockFetchResponse(paginatedResponse);

      const result = await apiClient.requestList<{ id: string }>('/v1/test');

      expect(result).toEqual([]);
    });
  });

  describe('handles legacy { items: T[] } format', () => {
    it('extracts items array from legacy wrapper', async () => {
      const legacyResponse = {
        items: [{ id: 'a' }, { id: 'b' }],
      };
      mockFetchResponse(legacyResponse);

      const result = await apiClient.requestList<{ id: string }>('/v1/test');

      expect(result).toEqual([{ id: 'a' }, { id: 'b' }]);
    });
  });

  describe('handles direct array response', () => {
    it('returns array when backend returns direct array', async () => {
      const directArray = [{ id: '1' }, { id: '2' }, { id: '3' }];
      mockFetchResponse(directArray);

      const result = await apiClient.requestList<{ id: string }>('/v1/test');

      expect(result).toEqual([{ id: '1' }, { id: '2' }, { id: '3' }]);
    });

    it('returns empty array for empty direct array', async () => {
      mockFetchResponse([]);

      const result = await apiClient.requestList<{ id: string }>('/v1/test');

      expect(result).toEqual([]);
    });
  });

  describe('fallback behavior', () => {
    it('returns empty array for unexpected response format', async () => {
      // Response that doesn't match any expected format
      mockFetchResponse({ unexpected: 'format', count: 5 });

      const result = await apiClient.requestList<{ id: string }>('/v1/test');

      expect(result).toEqual([]);
    });

    it('returns empty array for null response', async () => {
      mockFetchResponse(null);

      const result = await apiClient.requestList<{ id: string }>('/v1/test');

      expect(result).toEqual([]);
    });
  });

  describe('passes options correctly', () => {
    it('passes query parameters', async () => {
      mockFetchResponse([{ id: '1' }]);

      await apiClient.requestList<{ id: string }>('/v1/test?filter=active');

      expect(fetchMock).toHaveBeenCalledWith(
        expect.stringContaining('/v1/test?filter=active'),
        expect.any(Object)
      );
    });

    it('passes request options', async () => {
      mockFetchResponse([{ id: '1' }]);

      await apiClient.requestList<{ id: string }>('/v1/test', {
        headers: { 'X-Custom-Header': 'value' },
      });

      expect(fetchMock).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          headers: expect.objectContaining({
            'X-Custom-Header': 'value',
          }),
        })
      );
    });
  });

  describe('real-world scenarios', () => {
    it('handles tenants endpoint migrating to PaginatedResponse', async () => {
      // Simulates /v1/tenants returning PaginatedResponse instead of T[]
      const paginatedTenants = {
        schema_version: '1.0',
        data: [
          { id: 'tenant-1', name: 'Acme Corp' },
          { id: 'tenant-2', name: 'Globex Inc' },
        ],
        total: 2,
        page: 1,
        limit: 20,
        pages: 1,
      };
      mockFetchResponse(paginatedTenants);

      const result = await apiClient.requestList<{ id: string; name: string }>('/v1/tenants');

      expect(result).toHaveLength(2);
      expect(result[0].name).toBe('Acme Corp');
    });

    it('handles collections endpoint with PaginatedResponse', async () => {
      // This was the original bug - collections.map is not a function
      const paginatedCollections = {
        schema_version: '1.0',
        data: [
          { id: 'col-1', name: 'Documents', document_count: 5 },
        ],
        total: 1,
        page: 1,
        limit: 20,
        pages: 1,
      };
      mockFetchResponse(paginatedCollections);

      const result = await apiClient.requestList<{ id: string; name: string }>('/v1/collections');

      // This would have thrown "collections.map is not a function" before the fix
      expect(result.map(c => c.name)).toEqual(['Documents']);
    });
  });
});
