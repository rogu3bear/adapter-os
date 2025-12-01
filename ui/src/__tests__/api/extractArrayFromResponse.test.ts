/**
 * Tests for extractArrayFromResponse helper
 *
 * This helper defensively extracts arrays from various response formats,
 * protecting against backend pagination changes.
 */

import { describe, it, expect } from 'vitest';
import { extractArrayFromResponse } from '@/api/helpers';

describe('extractArrayFromResponse', () => {
  describe('PaginatedResponse format', () => {
    it('extracts data array from PaginatedResponse', () => {
      const response = {
        schema_version: '1.0',
        data: [{ id: '1' }, { id: '2' }],
        total: 2,
        page: 1,
        limit: 20,
        pages: 1,
      };

      const result = extractArrayFromResponse<{ id: string }>(response);
      expect(result).toEqual([{ id: '1' }, { id: '2' }]);
    });

    it('extracts empty array from PaginatedResponse with no data', () => {
      const response = {
        schema_version: '1.0',
        data: [],
        total: 0,
        page: 1,
        limit: 20,
        pages: 0,
      };

      const result = extractArrayFromResponse(response);
      expect(result).toEqual([]);
    });

    it('handles PaginatedResponse without schema_version', () => {
      const response = {
        data: [{ id: '1' }],
        total: 1,
        page: 1,
        limit: 20,
        pages: 1,
      };

      const result = extractArrayFromResponse<{ id: string }>(response);
      expect(result).toEqual([{ id: '1' }]);
    });
  });

  describe('Legacy { items: T[] } format', () => {
    it('extracts items array from legacy wrapper', () => {
      const response = {
        items: [{ name: 'a' }, { name: 'b' }],
      };

      const result = extractArrayFromResponse<{ name: string }>(response);
      expect(result).toEqual([{ name: 'a' }, { name: 'b' }]);
    });

    it('extracts empty items array', () => {
      const response = {
        items: [],
      };

      const result = extractArrayFromResponse(response);
      expect(result).toEqual([]);
    });
  });

  describe('Direct array response', () => {
    it('returns array when given direct array', () => {
      const response = [{ id: '1' }, { id: '2' }, { id: '3' }];

      const result = extractArrayFromResponse<{ id: string }>(response);
      expect(result).toEqual([{ id: '1' }, { id: '2' }, { id: '3' }]);
    });

    it('returns empty array when given empty array', () => {
      const response: unknown[] = [];

      const result = extractArrayFromResponse(response);
      expect(result).toEqual([]);
    });
  });

  describe('Fallback behavior', () => {
    it('returns empty array for null', () => {
      const result = extractArrayFromResponse(null);
      expect(result).toEqual([]);
    });

    it('returns empty array for undefined', () => {
      const result = extractArrayFromResponse(undefined);
      expect(result).toEqual([]);
    });

    it('returns empty array for plain object without data or items', () => {
      const response = { foo: 'bar', count: 5 };

      const result = extractArrayFromResponse(response);
      expect(result).toEqual([]);
    });

    it('returns empty array for string', () => {
      const result = extractArrayFromResponse('not an array');
      expect(result).toEqual([]);
    });

    it('returns empty array for number', () => {
      const result = extractArrayFromResponse(42);
      expect(result).toEqual([]);
    });

    it('returns empty array when data property is not an array', () => {
      const response = {
        data: 'not an array',
        total: 0,
      };

      const result = extractArrayFromResponse(response);
      expect(result).toEqual([]);
    });

    it('returns empty array when items property is not an array', () => {
      const response = {
        items: { nested: 'object' },
      };

      const result = extractArrayFromResponse(response);
      expect(result).toEqual([]);
    });
  });

  describe('Priority of formats', () => {
    it('prefers PaginatedResponse (data) over legacy (items)', () => {
      const response = {
        data: [{ source: 'paginated' }],
        items: [{ source: 'legacy' }],
      };

      const result = extractArrayFromResponse<{ source: string }>(response);
      expect(result).toEqual([{ source: 'paginated' }]);
    });

    it('falls back to legacy format when data is not an array', () => {
      const response = {
        data: 'invalid',
        items: [{ source: 'legacy' }],
      };

      const result = extractArrayFromResponse<{ source: string }>(response);
      expect(result).toEqual([{ source: 'legacy' }]);
    });
  });
});
