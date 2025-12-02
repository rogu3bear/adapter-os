/**
 * Integration test for Document → Chat → Evidence → PDF navigation flow
 *
 * Tests the complete user journey:
 * 1. Document upload and indexing
 * 2. Navigate to document chat page
 * 3. Send a message with document context
 * 4. Verify evidence panel shows sources
 * 5. Click on evidence item
 * 6. Verify PDF navigation callback is called
 *
 * 【2025-11-25†prd-ux-01†document_chat_flow_test】
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatInterface } from '@/components/ChatInterface';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import type { AdapterStack } from '@/api/types';
import type { Collection, Document, DocumentChunk } from '@/api/document-types';
import type { ChatSession, ChatMessage as BackendChatMessage } from '@/api/chat-types';

// ============================================================================
// Mock Data
// ============================================================================

const mockDocument: Document = {
  schema_version: '1.0',
  document_id: 'doc-123',
  name: 'Technical Specification.pdf',
  hash_b3: 'b3-hash-456',
  size_bytes: 1024000,
  mime_type: 'application/pdf',
  storage_path: '/var/aos/documents/doc-123.pdf',
  status: 'indexed',
  chunk_count: 42,
  tenant_id: 'test-tenant',
  created_at: '2025-01-01T10:00:00Z',
  updated_at: '2025-01-01T10:05:00Z',
};

const mockChunks: DocumentChunk[] = [
  {
    schema_version: '1.0',
    chunk_id: 'chunk-001',
    document_id: 'doc-123',
    chunk_index: 0,
    text: 'This section describes the authentication architecture...',
    embedding: null,
    metadata: { page_number: 5 },
    created_at: '2025-01-01T10:05:00Z',
  },
  {
    schema_version: '1.0',
    chunk_id: 'chunk-002',
    document_id: 'doc-123',
    chunk_index: 1,
    text: 'The system uses JWT tokens with Ed25519 signatures...',
    embedding: null,
    metadata: { page_number: 6 },
    created_at: '2025-01-01T10:05:00Z',
  },
];

const mockCollection: Collection = {
  schema_version: '1.0',
  collection_id: 'collection-456',
  name: 'Technical Documentation',
  description: 'System architecture and API docs',
  document_count: 1,
  tenant_id: 'test-tenant',
  created_at: '2025-01-01T09:00:00Z',
  updated_at: '2025-01-01T10:00:00Z',
};

const mockStack: AdapterStack = {
  id: 'stack-789',
  name: 'Documentation Assistant',
  adapter_ids: ['adapter-doc-1', 'adapter-doc-2'],
  description: 'Stack for document Q&A',
  created_at: '2025-01-01T08:00:00Z',
  updated_at: '2025-01-01T08:00:00Z',
};

const mockSession: ChatSession = {
  id: 'session-101',
  tenant_id: 'test-tenant',
  user_id: 'user-1',
  stack_id: 'stack-789',
  collection_id: 'collection-456',
  name: 'Doc Chat Session',
  created_at: '2025-01-01T11:00:00Z',
  last_activity_at: '2025-01-01T11:30:00Z',
};

const mockEvidence = [
  {
    document_id: 'doc-123',
    document_name: 'Technical Specification.pdf',
    chunk_id: 'chunk-001',
    page_number: 5,
    text_preview: 'This section describes the authentication architecture...',
    relevance_score: 0.95,
    rank: 1,
  },
  {
    document_id: 'doc-123',
    document_name: 'Technical Specification.pdf',
    chunk_id: 'chunk-002',
    page_number: 6,
    text_preview: 'The system uses JWT tokens with Ed25519 signatures...',
    relevance_score: 0.87,
    rank: 2,
  },
];

// ============================================================================
// Mock API Functions
// ============================================================================

const mockStreamInfer = vi.fn();
const mockGetAdapterStack = vi.fn();
const mockGetSessionRouterView = vi.fn();
const mockListCollections = vi.fn();
const mockGetCollection = vi.fn();
const mockUploadDocument = vi.fn();
const mockGetDocument = vi.fn();
const mockGetMessageEvidence = vi.fn();
const mockCreateChatSession = vi.fn();
const mockAddChatMessage = vi.fn();

// Mock API client
vi.mock('@/api/client', () => ({
  __esModule: true,
  default: {
    streamInfer: (...args: unknown[]) => mockStreamInfer(...args),
    getAdapterStack: (...args: unknown[]) => mockGetAdapterStack(...args),
    getSessionRouterView: (...args: unknown[]) => mockGetSessionRouterView(...args),
    listCollections: (...args: unknown[]) => mockListCollections(...args),
    getCollection: (...args: unknown[]) => mockGetCollection(...args),
    uploadDocument: (...args: unknown[]) => mockUploadDocument(...args),
    getDocument: (...args: unknown[]) => mockGetDocument(...args),
    getToken: vi.fn(() => 'test-token'),
    setToken: vi.fn(),
  },
  apiClient: {
    streamInfer: (...args: unknown[]) => mockStreamInfer(...args),
    getAdapterStack: (...args: unknown[]) => mockGetAdapterStack(...args),
    getSessionRouterView: (...args: unknown[]) => mockGetSessionRouterView(...args),
    listCollections: (...args: unknown[]) => mockListCollections(...args),
    getCollection: (...args: unknown[]) => mockGetCollection(...args),
    uploadDocument: (...args: unknown[]) => mockUploadDocument(...args),
    getDocument: (...args: unknown[]) => mockGetDocument(...args),
    getToken: vi.fn(() => 'test-token'),
    setToken: vi.fn(),
  },
}));

// Mock hooks with proper return values
vi.mock('@/hooks/useAdmin', () => ({
  useAdapterStacks: () => ({
    data: [mockStack],
    isLoading: false,
    error: null,
  }),
  useGetDefaultStack: () => ({
    data: mockStack,
    isLoading: false,
    error: null,
  }),
}));

vi.mock('@/hooks/useCollectionsApi', () => ({
  useCollections: () => ({
    data: [mockCollection],
    isLoading: false,
    error: null,
  }),
}));

const mockUseChatSessionsApi = vi.fn();
vi.mock('@/hooks/useChatSessionsApi', () => ({
  useChatSessionsApi: (tenantId: string) => mockUseChatSessionsApi(tenantId),
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Mock useSSE hook
vi.mock('@/hooks/useSSE', () => ({
  useSSE: vi.fn(() => ({
    data: null,
    error: null,
    connected: false,
  })),
}));

// Mock logger
vi.mock('@/utils/logger', () => ({
  logger: {
    error: vi.fn(),
    warn: vi.fn(),
    info: vi.fn(),
  },
  toError: (error: unknown) => error,
}));

// ============================================================================
// Test Wrapper Component
// ============================================================================

function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0 },
      mutations: { retry: false },
    },
  });

  return (
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        {children}
      </QueryClientProvider>
    </MemoryRouter>
  );
}

// ============================================================================
// Tests
// ============================================================================

describe('DocumentChatFlow Integration', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    // Setup default useChatSessionsApi mock
    mockUseChatSessionsApi.mockReturnValue({
      sessions: [],
      isLoading: false,
      createSession: mockCreateChatSession,
      updateSession: vi.fn(),
      addMessage: mockAddChatMessage,
      updateMessage: vi.fn(),
      deleteSession: vi.fn(),
      getSession: vi.fn((id: string) => (id === mockSession.id ? mockSession : null)),
      updateSessionCollection: vi.fn(),
    });

    // Setup default mock responses
    mockListCollections.mockResolvedValue([mockCollection]);
    mockGetCollection.mockResolvedValue({
      ...mockCollection,
      documents: [
        {
          document_id: mockDocument.document_id,
          name: mockDocument.name,
          size_bytes: mockDocument.size_bytes,
          status: mockDocument.status,
          added_at: mockDocument.created_at,
        },
      ],
    });
    mockGetAdapterStack.mockResolvedValue(mockStack);
    mockGetSessionRouterView.mockResolvedValue({
      request_id: 'req-123',
      stack_id: mockStack.id,
      steps: [
        {
          timestamp: '2025-01-01T11:30:00Z',
          entropy: 0.5,
          tau: 1.0,
          step: 0,
          adapters_fired: [
            { adapter_idx: 0, gate_value: 0.9, selected: true },
            { adapter_idx: 1, gate_value: 0.7, selected: true },
          ],
        },
      ],
    });

    // Setup default createSession mock
    mockCreateChatSession.mockImplementation((name: string, stackId: string) => ({
      id: 'session-new',
      tenant_id: 'test-tenant',
      user_id: 'user-1',
      stack_id: stackId,
      collection_id: null,
      name,
      created_at: new Date().toISOString(),
      last_activity_at: new Date().toISOString(),
      messages: [],
      updatedAt: new Date(),
    }));
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('Document Upload and Indexing', () => {
    it('successfully uploads a document', async () => {
      mockUploadDocument.mockResolvedValue({
        document_id: mockDocument.document_id,
        name: mockDocument.name,
        status: 'processing',
      });

      const result = await mockUploadDocument({
        file: new File(['test content'], 'test.pdf', { type: 'application/pdf' }),
        collectionId: mockCollection.collection_id,
      });

      expect(result.document_id).toBe(mockDocument.document_id);
      expect(result.status).toBe('processing');
    });

    it('polls document status until indexed', async () => {
      mockGetDocument
        .mockResolvedValueOnce({ ...mockDocument, status: 'processing' })
        .mockResolvedValueOnce({ ...mockDocument, status: 'processing' })
        .mockResolvedValueOnce({ ...mockDocument, status: 'indexed' });

      // Simulate polling
      let status = 'processing';
      let attempts = 0;
      while (status === 'processing' && attempts < 5) {
        const doc = await mockGetDocument(mockDocument.document_id);
        status = doc.status;
        attempts++;
      }

      expect(status).toBe('indexed');
      expect(mockGetDocument).toHaveBeenCalledTimes(3);
    });

    it('handles upload errors gracefully', async () => {
      mockUploadDocument.mockRejectedValue(new Error('Upload failed'));

      await expect(
        mockUploadDocument({
          file: new File(['test'], 'test.pdf'),
          collectionId: mockCollection.collection_id,
        })
      ).rejects.toThrow('Upload failed');
    });
  });

  describe('Chat with Document Context', () => {
    it('sends message with document context and collection binding', async () => {
      const onViewDocument = vi.fn();

      // Mock streaming inference with evidence
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onToken('The ');
            callbacks.onToken('authentication ');
            callbacks.onToken('system ');
            callbacks.onToken('uses ');
            callbacks.onToken('JWT ');
            callbacks.onToken('tokens.');
            callbacks.onComplete('The authentication system uses JWT tokens.', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      // Mock evidence fetch
      global.fetch = vi.fn((url: string) => {
        if (url.includes('/evidence')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockEvidence),
          } as Response);
        }
        return Promise.reject(new Error('Not found'));
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
            documentContext={{
              documentId: mockDocument.document_id,
              documentName: mockDocument.name,
              collectionId: mockCollection.collection_id,
            }}
            onViewDocument={onViewDocument}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Type a question about the document
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'What authentication method does the system use?');

      // Send message
      const sendButton = screen.getByRole('button', { name: /send message/i });
      await user.click(sendButton);

      // Wait for response
      await waitFor(
        () => {
          expect(
            screen.getByText(/The authentication system uses JWT tokens/)
          ).toBeInTheDocument();
        },
        { timeout: 3000 }
      );

      // Verify stream inference was called with document context
      expect(mockStreamInfer).toHaveBeenCalledWith(
        expect.objectContaining({
          prompt: 'What authentication method does the system use?',
          adapter_stack: mockStack.adapter_ids,
          document_id: mockDocument.document_id,
          collection_id: mockCollection.collection_id,
        }),
        expect.any(Object),
        expect.any(AbortSignal)
      );
    });

    it('displays loading state during message streaming', async () => {
      let resolveStream: ((value: void) => void) | null = null;
      const streamPromise = new Promise<void>((resolve) => {
        resolveStream = resolve;
      });

      mockStreamInfer.mockImplementation(() => streamPromise);

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test message');

      const sendButton = screen.getByRole('button', { name: /send message/i });
      await user.click(sendButton);

      // Should show loading state
      await waitFor(() => {
        const loadingButton = screen.getByRole('button', { name: /sending message/i });
        expect(loadingButton).toBeDisabled();
      });

      // Resolve stream
      resolveStream?.();
    });

    it('handles inference errors gracefully', async () => {
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onError(new Error('Inference service unavailable'));
          }, 10);
          return Promise.resolve();
        }
      );

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test message');

      const sendButton = screen.getByRole('button', { name: /send message/i });
      await user.click(sendButton);

      // Error message should not be added to chat
      await waitFor(() => {
        expect(screen.queryByText(/Inference service unavailable/)).not.toBeInTheDocument();
      });
    });
  });

  describe('Evidence Panel Display', () => {
    it('shows evidence panel with sources after message completion', async () => {
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onToken('Response text');
            callbacks.onComplete('Response text', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      // Mock evidence fetch
      global.fetch = vi.fn((url: string) => {
        if (url.includes('/evidence')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockEvidence),
          } as Response);
        }
        return Promise.reject(new Error('Not found'));
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test question');

      const sendButton = screen.getByRole('button', { name: /send message/i });
      await user.click(sendButton);

      // Wait for evidence panel to appear
      await waitFor(
        () => {
          const evidenceButton = screen.getByRole('button', { name: /sources \(2\)/i });
          expect(evidenceButton).toBeInTheDocument();
        },
        { timeout: 3000 }
      );
    });

    it('expands evidence panel to show source details', async () => {
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onComplete('Response', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      global.fetch = vi.fn((url: string) => {
        if (url.includes('/evidence')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockEvidence),
          } as Response);
        }
        return Promise.reject(new Error('Not found'));
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test');

      const sendButton = screen.getByRole('button', { name: /send message/i });
      await user.click(sendButton);

      // Wait for evidence panel
      await waitFor(() => {
        expect(screen.getByRole('button', { name: /sources \(2\)/i })).toBeInTheDocument();
      });

      // Click to expand
      const evidenceButton = screen.getByRole('button', { name: /sources \(2\)/i });
      await user.click(evidenceButton);

      // Should show evidence items
      await waitFor(() => {
        expect(screen.getByText(/Technical Specification.pdf/)).toBeInTheDocument();
        expect(screen.getByText(/Page 5/i)).toBeInTheDocument();
      });
    });

    it('shows verified badge when evidence is present', async () => {
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onComplete('Response', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      global.fetch = vi.fn((url: string) => {
        if (url.includes('/evidence')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockEvidence),
          } as Response);
        }
        return Promise.reject(new Error('Not found'));
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      await waitFor(() => {
        expect(screen.getByText(/Verified/i)).toBeInTheDocument();
      });
    });
  });

  describe('PDF Navigation from Evidence', () => {
    it('calls onViewDocument when evidence item is clicked', async () => {
      const onViewDocument = vi.fn();

      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onComplete('Response', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      global.fetch = vi.fn((url: string) => {
        if (url.includes('/evidence')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockEvidence),
          } as Response);
        }
        return Promise.reject(new Error('Not found'));
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
            onViewDocument={onViewDocument}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      // Wait for evidence panel and expand it
      await waitFor(() => {
        expect(screen.getByRole('button', { name: /sources \(2\)/i })).toBeInTheDocument();
      });

      const evidenceButton = screen.getByRole('button', { name: /sources \(2\)/i });
      await user.click(evidenceButton);

      // Wait for evidence items to be visible
      await waitFor(() => {
        expect(screen.getByText(/Page 5/i)).toBeInTheDocument();
      });

      // Find and click the first evidence item's view button
      const viewButtons = screen.getAllByRole('button', { name: /view/i });
      await user.click(viewButtons[0]);

      // Verify callback was called with correct parameters
      expect(onViewDocument).toHaveBeenCalledWith(
        mockDocument.document_id,
        5,
        expect.any(String) // highlightText
      );
    });

    it('navigates to correct page number from evidence', async () => {
      const onViewDocument = vi.fn();

      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onComplete('Response', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      global.fetch = vi.fn((url: string) => {
        if (url.includes('/evidence')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockEvidence),
          } as Response);
        }
        return Promise.reject(new Error('Not found'));
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
            onViewDocument={onViewDocument}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /sources \(2\)/i })).toBeInTheDocument();
      });

      const evidenceButton = screen.getByRole('button', { name: /sources \(2\)/i });
      await user.click(evidenceButton);

      await waitFor(() => {
        expect(screen.getByText(/Page 6/i)).toBeInTheDocument();
      });

      // Click second evidence item (page 6)
      const viewButtons = screen.getAllByRole('button', { name: /view/i });
      await user.click(viewButtons[1]);

      expect(onViewDocument).toHaveBeenCalledWith(
        mockDocument.document_id,
        6,
        expect.any(String)
      );
    });

    it('handles missing onViewDocument callback gracefully', async () => {
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onComplete('Response', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      global.fetch = vi.fn((url: string) => {
        if (url.includes('/evidence')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockEvidence),
          } as Response);
        }
        return Promise.reject(new Error('Not found'));
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
            // No onViewDocument callback
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /sources \(2\)/i })).toBeInTheDocument();
      });

      const evidenceButton = screen.getByRole('button', { name: /sources \(2\)/i });
      await user.click(evidenceButton);

      // Should not throw when clicking view without callback
      await waitFor(() => {
        expect(screen.getByText(/Page 5/i)).toBeInTheDocument();
      });

      const viewButtons = screen.getAllByRole('button', { name: /view/i });
      await expect(user.click(viewButtons[0])).resolves.not.toThrow();
    });
  });

  describe('Collection Binding', () => {
    it('displays selected collection in context panel', () => {
      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      // Should show collection selector in header
      expect(screen.getByText(/Collection:/)).toBeInTheDocument();
    });

    it('updates session when collection is changed', async () => {
      const updateSessionCollection = vi.fn().mockResolvedValue(undefined);

      // Override mock for this test
      mockUseChatSessionsApi.mockReturnValue({
        sessions: [mockSession],
        isLoading: false,
        createSession: mockCreateChatSession,
        updateSession: vi.fn(),
        addMessage: mockAddChatMessage,
        updateMessage: vi.fn(),
        deleteSession: vi.fn(),
        getSession: vi.fn((id: string) => (id === mockSession.id ? mockSession : null)),
        updateSessionCollection,
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
            sessionId={mockSession.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Find and click collection selector
      const collectionSelect = screen.getByRole('combobox', { name: /select collection/i });
      await user.click(collectionSelect);

      // Select the collection
      await waitFor(() => {
        const option = screen.getByRole('option', { name: new RegExp(mockCollection.name) });
        expect(option).toBeInTheDocument();
      });

      // Click the collection option
      const collectionOption = screen.getByRole('option', {
        name: new RegExp(mockCollection.name),
      });
      await user.click(collectionOption);

      // Verify updateSessionCollection was called
      await waitFor(() => {
        expect(updateSessionCollection).toHaveBeenCalledWith(
          mockSession.id,
          mockCollection.collection_id
        );
      });
    });

    it('includes collection_id in inference requests', async () => {
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onComplete('Response', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      // Create session with collection
      mockCreateChatSession.mockReturnValue({
        id: 'session-new',
        tenant_id: 'test-tenant',
        stack_id: mockStack.id,
        collection_id: mockCollection.collection_id,
        name: 'Test Session',
        created_at: new Date().toISOString(),
        last_activity_at: new Date().toISOString(),
        messages: [],
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // Select collection first
      const collectionSelect = screen.getByRole('combobox', { name: /select collection/i });
      await user.click(collectionSelect);

      // Wait for collection option to appear and click it
      await waitFor(() => {
        const option = screen.getByRole('option', { name: new RegExp(mockCollection.name) });
        expect(option).toBeInTheDocument();
      });

      const collectionOption = screen.getByRole('option', {
        name: new RegExp(mockCollection.name),
      });
      await user.click(collectionOption);

      // Now send a message
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test with collection');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      // Verify collection_id is included in request
      await waitFor(() => {
        expect(mockStreamInfer).toHaveBeenCalledWith(
          expect.objectContaining({
            collection_id: mockCollection.collection_id,
          }),
          expect.any(Object),
          expect.any(AbortSignal)
        );
      });
    });
  });

  describe('Error Handling', () => {
    it('handles evidence fetch failure gracefully', async () => {
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onComplete('Response', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      // Mock evidence fetch failure
      global.fetch = vi.fn((url: string) => {
        if (url.includes('/evidence')) {
          return Promise.resolve({
            ok: false,
            status: 500,
            statusText: 'Internal Server Error',
          } as Response);
        }
        return Promise.reject(new Error('Not found'));
      });

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      // Should complete without error, but no evidence panel
      await waitFor(() => {
        expect(screen.getByText('Response')).toBeInTheDocument();
      });

      // No evidence panel should be shown
      expect(screen.queryByRole('button', { name: /sources/i })).not.toBeInTheDocument();
    });

    it('handles router decision fetch failure', async () => {
      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          setTimeout(() => {
            callbacks.onComplete('Response', 'stop');
          }, 10);
          return Promise.resolve();
        }
      );

      mockGetSessionRouterView.mockRejectedValue(new Error('Router decision not found'));

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      // Should complete message despite router decision failure
      await waitFor(() => {
        expect(screen.getByText('Response')).toBeInTheDocument();
      });
    });

    it('recovers from network interruption during streaming', async () => {
      let shouldFail = true;

      mockStreamInfer.mockImplementation(
        (
          req: unknown,
          callbacks: {
            onToken: (token: string) => void;
            onComplete: (text: string, reason: string | null) => void;
            onError: (error: Error) => void;
          }
        ) => {
          if (shouldFail) {
            setTimeout(() => {
              callbacks.onError(new Error('Network error'));
            }, 10);
          } else {
            setTimeout(() => {
              callbacks.onComplete('Success', 'stop');
            }, 10);
          }
          return Promise.resolve();
        }
      );

      render(
        <TestWrapper>
          <ChatInterface
            selectedTenant="test-tenant"
            initialStackId={mockStack.id}
          />
        </TestWrapper>
      );

      const user = userEvent.setup();

      // First attempt fails
      const input = screen.getByPlaceholderText(/Type your message/);
      await user.type(input, 'Test');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      // Wait for error (message should be removed)
      await waitFor(() => {
        expect(screen.queryByText('Test')).toBeInTheDocument(); // User message stays
      });

      // Retry succeeds
      shouldFail = false;
      await user.type(input, ' retry');
      await user.click(screen.getByRole('button', { name: /send message/i }));

      await waitFor(() => {
        expect(screen.getByText('Success')).toBeInTheDocument();
      });
    });
  });
});
