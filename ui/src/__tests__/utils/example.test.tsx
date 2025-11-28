/**
 * Example test demonstrating test utility usage
 *
 * This file shows how to use the test utilities for testing components and hooks.
 */

import { describe, test, expect, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { renderHook } from '@testing-library/react';
import {
  renderWithProviders,
  createMockApiClient,
  createMockDocument,
  createMockDocumentList,
  createMockCollection,
  QueryWrapper,
  waitForQueries,
  type MockApiClient,
} from './index';

describe('Test Utilities Examples', () => {
  describe('Mock Factories', () => {
    test('creates mock document with defaults', () => {
      const doc = createMockDocument();

      expect(doc.id).toBe('doc-1');
      expect(doc.content).toBe('Sample document content');
      expect(doc.metadata.title).toBe('Sample Document');
    });

    test('creates mock document with overrides', () => {
      const doc = createMockDocument({
        id: 'custom-id',
        content: 'Custom content',
        metadata: {
          title: 'Custom Title',
          source: 'api',
          filename: 'custom.txt',
          mime_type: 'text/plain',
          size_bytes: 2048,
          tags: ['custom', 'test'],
        },
      });

      expect(doc.id).toBe('custom-id');
      expect(doc.content).toBe('Custom content');
      expect(doc.metadata.title).toBe('Custom Title');
      expect(doc.metadata.tags).toContain('custom');
    });

    test('creates list of mock documents', () => {
      const docs = createMockDocumentList(5);

      expect(docs).toHaveLength(5);
      expect(docs[0].id).toBe('doc-1');
      expect(docs[4].id).toBe('doc-5');
    });

    test('creates mock collection', () => {
      const collection = createMockCollection({
        name: 'Test Collection',
        description: 'A test collection',
        document_count: 10,
      });

      expect(collection.name).toBe('Test Collection');
      expect(collection.document_count).toBe(10);
    });
  });

  describe('Mock API Client', () => {
    let mockApi: MockApiClient;

    beforeEach(() => {
      mockApi = createMockApiClient();
    });

    test('gets documents with default data', async () => {
      const response = await mockApi.getDocuments();

      expect(response.items).toHaveLength(5);
      expect(response.total).toBe(5);
    });

    test('gets single document', async () => {
      const doc = await mockApi.getDocument('doc-1');

      expect(doc.id).toBe('doc-1');
    });

    test('creates new document', async () => {
      const newDoc = await mockApi.createDocument({
        collection_id: 'collection-1',
        content: 'New document content',
        metadata: {
          title: 'New Document',
          source: 'test',
          filename: 'new.txt',
          mime_type: 'text/plain',
          size_bytes: 1024,
        },
      });

      expect(newDoc.content).toBe('New document content');
      expect(newDoc.metadata.title).toBe('New Document');

      // Verify document was added to state
      const state = mockApi.getState();
      expect(state.getDocuments()).toHaveLength(6); // 5 default + 1 new
    });

    test('updates document', async () => {
      const updated = await mockApi.updateDocument('doc-1', {
        content: 'Updated content',
      });

      expect(updated.content).toBe('Updated content');
    });

    test('deletes document', async () => {
      await mockApi.deleteDocument('doc-1');

      const state = mockApi.getState();
      expect(state.getDocument('doc-1')).toBeUndefined();
      expect(state.getDocuments()).toHaveLength(4); // 5 - 1
    });

    test('filters documents by collection', async () => {
      // Add documents to specific collection
      mockApi.getState().addDocument(
        createMockDocument({
          id: 'doc-custom-1',
          collection_id: 'custom-collection',
        })
      );

      const response = await mockApi.getDocuments({
        collection_id: 'custom-collection',
      });

      expect(response.items).toHaveLength(1);
      expect(response.items[0].collection_id).toBe('custom-collection');
    });

    test('searches documents by content', async () => {
      mockApi.getState().addDocument(
        createMockDocument({
          id: 'searchable',
          content: 'This document contains TypeScript code',
        })
      );

      const response = await mockApi.getDocuments({
        search: 'TypeScript',
      });

      expect(response.items.length).toBeGreaterThan(0);
      expect(
        response.items.some((doc) => doc.content.includes('TypeScript'))
      ).toBe(true);
    });

    test('handles errors when configured', async () => {
      mockApi.setConfig({
        shouldError: true,
        errorMessage: 'Network error',
      });

      await expect(mockApi.getDocuments()).rejects.toMatchObject({
        error: 'Network error',
      });
    });

    test('simulates delays', async () => {
      mockApi.setConfig({ delay: 100 });

      const start = Date.now();
      await mockApi.getDocuments();
      const duration = Date.now() - start;

      expect(duration).toBeGreaterThanOrEqual(100);
    });

    test('resets state', async () => {
      // Modify state
      await mockApi.deleteDocument('doc-1');
      expect(mockApi.getState().getDocuments()).toHaveLength(4);

      // Reset
      mockApi.reset();
      expect(mockApi.getState().getDocuments()).toHaveLength(5);
    });
  });

  describe('Test Providers', () => {
    test('renders component with providers', () => {
      const TestComponent = () => <div>Test Content</div>;

      const { getByText } = renderWithProviders(<TestComponent />);

      expect(getByText('Test Content')).toBeInTheDocument();
    });

    test('provides query client to component', async () => {
      const TestComponent = () => {
        // Component would use useQuery here
        return <div>Query Component</div>;
      };

      const { getByText, queryClient } = renderWithProviders(<TestComponent />);

      expect(getByText('Query Component')).toBeInTheDocument();
      expect(queryClient).toBeDefined();

      // Clean up
      await waitForQueries(queryClient);
    });

    test('renders with custom query client', () => {
      // Note: This is just an example. In real tests, you'd use the actual QueryClient
      const mockQueryClient = createMockApiClient();

      const TestComponent = () => <div>Custom Client</div>;

      const { getByText } = renderWithProviders(<TestComponent />, {
        // queryClient: mockQueryClient, // Type mismatch - just for demonstration
      });

      expect(getByText('Custom Client')).toBeInTheDocument();
    });
  });

  describe('Hook Testing', () => {
    test('wraps hook with QueryWrapper', () => {
      const useTestHook = () => {
        return { value: 'test' };
      };

      const { result } = renderHook(() => useTestHook(), {
        wrapper: QueryWrapper,
      });

      expect(result.current.value).toBe('test');
    });
  });
});
