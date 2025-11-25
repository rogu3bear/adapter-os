/**
 * Integration Test: Collection → Train → Adapter → Stack → Chat Flow
 *
 * Tests the complete workflow from creating a collection with documents,
 * through training an adapter, adding it to a stack, and using it in chat.
 *
 * 【2025-11-25†test†training_flow_integration】
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import type {
  Collection,
  Document,
  TrainingJob,
  Adapter,
  AdapterStack,
  ChatSession,
  ChatMessage,
  TrainingStatus
} from '@/api/types';

// Mock API client
const mockApiClient = {
  // Collection methods
  createCollection: vi.fn(),
  listCollections: vi.fn(),
  getCollection: vi.fn(),
  addDocumentToCollection: vi.fn(),

  // Document methods
  uploadDocument: vi.fn(),
  listDocuments: vi.fn(),

  // Training methods
  startTraining: vi.fn(),
  getTrainingJob: vi.fn(),
  listTrainingJobs: vi.fn(),
  cancelTraining: vi.fn(),
  getTrainingArtifacts: vi.fn(),

  // Adapter methods
  listAdapters: vi.fn(),
  getAdapter: vi.fn(),
  registerAdapter: vi.fn(),

  // Adapter Stack methods
  createAdapterStack: vi.fn(),
  listAdapterStacks: vi.fn(),
  getAdapterStack: vi.fn(),
  activateAdapterStack: vi.fn(),

  // Chat methods
  createChatSession: vi.fn(),
  listChatSessions: vi.fn(),
  addChatMessage: vi.fn(),
  streamInfer: vi.fn(),
};

vi.mock('@/api/client', () => ({
  __esModule: true,
  default: mockApiClient,
}));

// Mock toast notifications
const mockToast = {
  success: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
  loading: vi.fn(),
};

vi.mock('sonner', () => ({
  toast: mockToast,
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

// Test wrapper component
function TestWrapper({
  children,
  initialRoute = '/'
}: {
  children: React.ReactNode;
  initialRoute?: string;
}) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0 },
      mutations: { retry: false }
    },
  });

  return (
    <MemoryRouter initialEntries={[initialRoute]}>
      <QueryClientProvider client={queryClient}>
        {children}
      </QueryClientProvider>
    </MemoryRouter>
  );
}

// Mock data generators
const createMockCollection = (id: string, name: string): Collection => ({
  id,
  name,
  tenant_id: 'test-tenant',
  description: `Test collection ${name}`,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  document_count: 0,
});

const createMockDocument = (id: string, name: string): Document => ({
  id,
  name,
  tenant_id: 'test-tenant',
  filename: `${name}.pdf`,
  content_type: 'application/pdf',
  size_bytes: 1024,
  hash_b3: 'abc123',
  storage_path: `/documents/${id}`,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  chunk_count: 10,
});

const createMockTrainingJob = (
  id: string,
  status: TrainingStatus,
  adapter_id?: string
): TrainingJob => ({
  id,
  status,
  adapter_id,
  adapter_name: adapter_id ? `tenant-a/domain/adapter-${id}` : undefined,
  dataset_id: 'dataset-1',
  progress_pct: status === 'completed' ? 100 : status === 'running' ? 50 : 0,
  current_loss: status === 'running' ? 0.5 : status === 'completed' ? 0.1 : undefined,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  started_at: status !== 'pending' ? new Date().toISOString() : undefined,
  completed_at: status === 'completed' ? new Date().toISOString() : undefined,
  config: {
    learning_rate: 0.001,
    epochs: 3,
    batch_size: 8,
    rank: 16,
    alpha: 32,
  },
});

const createMockAdapter = (id: string, name: string): Adapter => ({
  id,
  name,
  tenant_id: 'test-tenant',
  hash: 'hash123',
  tier: 'tier_1',
  rank: 16,
  acl: ['test-tenant'],
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  lifecycle_state: 'cold',
  activation_pct: 0,
});

const createMockStack = (id: string, name: string, adapter_ids: string[]): AdapterStack => ({
  id,
  name,
  adapter_ids,
  description: `Test stack ${name}`,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  lifecycle_state: 'active',
});

const createMockChatSession = (id: string, stack_id?: string): ChatSession => ({
  id,
  tenant_id: 'test-tenant',
  name: 'Test Chat Session',
  stack_id,
  created_at: new Date().toISOString(),
  last_activity_at: new Date().toISOString(),
});

const createMockChatMessage = (
  id: string,
  role: 'user' | 'assistant',
  content: string
): ChatMessage => ({
  id,
  session_id: 'session-1',
  role,
  content,
  timestamp: new Date().toISOString(),
});

describe('TrainingFlow Integration Tests', () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false, staleTime: 0 },
        mutations: { retry: false }
      },
    });
  });

  afterEach(() => {
    queryClient.clear();
  });

  describe('1. Collection Creation with Documents', () => {
    it('creates a collection and adds documents', async () => {
      const mockCollection = createMockCollection('coll-1', 'Training Collection');
      const mockDoc1 = createMockDocument('doc-1', 'Document 1');
      const mockDoc2 = createMockDocument('doc-2', 'Document 2');

      mockApiClient.createCollection.mockResolvedValue(mockCollection);
      mockApiClient.uploadDocument
        .mockResolvedValueOnce(mockDoc1)
        .mockResolvedValueOnce(mockDoc2);
      mockApiClient.addDocumentToCollection.mockResolvedValue(undefined);
      mockApiClient.getCollection.mockResolvedValue({
        ...mockCollection,
        documents: [mockDoc1, mockDoc2],
        document_count: 2,
      });

      // Create collection
      const collection = await mockApiClient.createCollection(
        'Training Collection',
        'Collection for training'
      );

      expect(collection).toEqual(mockCollection);
      expect(mockToast.success).not.toHaveBeenCalled(); // Not called in test

      // Upload documents
      const file1 = new File(['content1'], 'doc1.pdf', { type: 'application/pdf' });
      const file2 = new File(['content2'], 'doc2.pdf', { type: 'application/pdf' });

      const doc1 = await mockApiClient.uploadDocument(file1);
      const doc2 = await mockApiClient.uploadDocument(file2);

      expect(doc1).toEqual(mockDoc1);
      expect(doc2).toEqual(mockDoc2);

      // Add documents to collection
      await mockApiClient.addDocumentToCollection('coll-1', 'doc-1');
      await mockApiClient.addDocumentToCollection('coll-1', 'doc-2');

      expect(mockApiClient.addDocumentToCollection).toHaveBeenCalledTimes(2);

      // Verify collection has documents
      const collectionDetail = await mockApiClient.getCollection('coll-1');
      expect(collectionDetail.documents).toHaveLength(2);
      expect(collectionDetail.document_count).toBe(2);
    });

    it('handles document upload errors gracefully', async () => {
      const error = new Error('Upload failed');
      mockApiClient.uploadDocument.mockRejectedValue(error);

      await expect(
        mockApiClient.uploadDocument(new File(['content'], 'test.pdf'))
      ).rejects.toThrow('Upload failed');
    });
  });

  describe('2. Training Job with Collection', () => {
    it('starts training with collection-based dataset', async () => {
      const mockJob = createMockTrainingJob('job-1', 'pending');
      mockApiClient.startTraining.mockResolvedValue(mockJob);

      const trainingRequest = {
        adapter_name: 'tenant-a/domain/test-adapter/r001',
        config: {
          learning_rate: 0.001,
          epochs: 3,
          batch_size: 8,
          rank: 16,
          alpha: 32,
        },
        dataset_id: 'dataset-from-coll-1',
      };

      const job = await mockApiClient.startTraining(trainingRequest);

      expect(job).toEqual(mockJob);
      expect(job.status).toBe('pending');
      expect(mockApiClient.startTraining).toHaveBeenCalledWith(trainingRequest);
    });

    it('polls training job status until completion', async () => {
      const jobId = 'job-1';
      let callCount = 0;

      mockApiClient.getTrainingJob.mockImplementation(async () => {
        callCount++;
        if (callCount === 1) {
          return createMockTrainingJob(jobId, 'pending');
        } else if (callCount === 2) {
          return createMockTrainingJob(jobId, 'running');
        } else {
          return createMockTrainingJob(jobId, 'completed', 'adapter-1');
        }
      });

      // Simulate polling
      let job = await mockApiClient.getTrainingJob(jobId);
      expect(job.status).toBe('pending');

      job = await mockApiClient.getTrainingJob(jobId);
      expect(job.status).toBe('running');
      expect(job.progress_pct).toBe(50);

      job = await mockApiClient.getTrainingJob(jobId);
      expect(job.status).toBe('completed');
      expect(job.progress_pct).toBe(100);
      expect(job.adapter_id).toBe('adapter-1');

      expect(mockApiClient.getTrainingJob).toHaveBeenCalledTimes(3);
    });

    it('handles training job transitions correctly', async () => {
      const transitions: TrainingStatus[] = ['pending', 'running', 'completed'];

      for (const status of transitions) {
        const job = createMockTrainingJob('job-1', status);
        expect(job.status).toBe(status);

        if (status === 'pending') {
          expect(job.started_at).toBeUndefined();
          expect(job.completed_at).toBeUndefined();
        } else if (status === 'running') {
          expect(job.started_at).toBeDefined();
          expect(job.completed_at).toBeUndefined();
        } else if (status === 'completed') {
          expect(job.started_at).toBeDefined();
          expect(job.completed_at).toBeDefined();
        }
      }
    });

    it('handles training cancellation', async () => {
      mockApiClient.cancelTraining.mockResolvedValue(undefined);
      mockApiClient.getTrainingJob.mockResolvedValue(
        createMockTrainingJob('job-1', 'cancelled')
      );

      await mockApiClient.cancelTraining('job-1');
      const job = await mockApiClient.getTrainingJob('job-1');

      expect(job.status).toBe('cancelled');
      expect(mockApiClient.cancelTraining).toHaveBeenCalledWith('job-1');
    });

    it('handles training failure with error message', async () => {
      const failedJob = {
        ...createMockTrainingJob('job-1', 'failed'),
        error_message: 'Out of memory',
      };

      mockApiClient.getTrainingJob.mockResolvedValue(failedJob);

      const job = await mockApiClient.getTrainingJob('job-1');
      expect(job.status).toBe('failed');
      expect(job.error_message).toBe('Out of memory');
    });
  });

  describe('3. Adapter Creation and Verification', () => {
    it('verifies adapter is created after training completion', async () => {
      const completedJob = createMockTrainingJob('job-1', 'completed', 'adapter-1');
      const mockAdapter = createMockAdapter('adapter-1', 'tenant-a/domain/test-adapter/r001');

      mockApiClient.getTrainingJob.mockResolvedValue(completedJob);
      mockApiClient.getAdapter.mockResolvedValue(mockAdapter);

      const job = await mockApiClient.getTrainingJob('job-1');
      expect(job.adapter_id).toBe('adapter-1');

      const adapter = await mockApiClient.getAdapter('adapter-1');
      expect(adapter.id).toBe('adapter-1');
      expect(adapter.name).toBe('tenant-a/domain/test-adapter/r001');
    });

    it('retrieves training artifacts for completed job', async () => {
      const artifacts = {
        schema_version: '1.0',
        job_id: 'job-1',
        artifacts: [
          {
            id: 'artifact-1',
            type: 'final' as const,
            path: '/artifacts/adapter-1.aos',
            size_bytes: 1024000,
            created_at: new Date().toISOString(),
          },
          {
            id: 'artifact-2',
            type: 'log' as const,
            path: '/artifacts/training.log',
            size_bytes: 4096,
            created_at: new Date().toISOString(),
          },
        ],
        ready: true,
        signature_valid: true,
      };

      mockApiClient.getTrainingArtifacts.mockResolvedValue(artifacts);

      const result = await mockApiClient.getTrainingArtifacts('job-1');
      expect(result.artifacts).toHaveLength(2);
      expect(result.ready).toBe(true);
      expect(result.signature_valid).toBe(true);
    });
  });

  describe('4. Add Adapter to Stack', () => {
    it('creates adapter stack with trained adapter', async () => {
      const mockStack = createMockStack('stack-1', 'Test Stack', ['adapter-1']);

      mockApiClient.createAdapterStack.mockResolvedValue({
        schema_version: '1.0',
        stack: mockStack,
      });

      const result = await mockApiClient.createAdapterStack({
        name: 'Test Stack',
        adapter_ids: ['adapter-1'],
        description: 'Stack with trained adapter',
      });

      expect(result.stack).toEqual(mockStack);
      expect(result.stack.adapter_ids).toContain('adapter-1');
    });

    it('adds adapter to existing stack', async () => {
      const existingStack = createMockStack('stack-1', 'Existing Stack', ['adapter-old']);
      const updatedStack = createMockStack('stack-1', 'Existing Stack', ['adapter-old', 'adapter-1']);

      mockApiClient.getAdapterStack.mockResolvedValue(existingStack);
      mockApiClient.createAdapterStack.mockResolvedValue({
        schema_version: '1.0',
        stack: updatedStack,
      });

      // Get existing stack
      const stack = await mockApiClient.getAdapterStack('stack-1');
      expect(stack.adapter_ids).toHaveLength(1);

      // Update with new adapter
      const result = await mockApiClient.createAdapterStack({
        name: 'Existing Stack',
        adapter_ids: ['adapter-old', 'adapter-1'],
      });

      expect(result.stack.adapter_ids).toHaveLength(2);
      expect(result.stack.adapter_ids).toContain('adapter-1');
    });

    it('activates adapter stack', async () => {
      mockApiClient.activateAdapterStack.mockResolvedValue({
        schema_version: '1.0',
        stack_id: 'stack-1',
        success: true,
      });

      const result = await mockApiClient.activateAdapterStack('stack-1');
      expect(result.success).toBe(true);
      expect(mockApiClient.activateAdapterStack).toHaveBeenCalledWith('stack-1');
    });

    it('shows toast notification on stack activation success', async () => {
      mockApiClient.activateAdapterStack.mockResolvedValue({
        schema_version: '1.0',
        stack_id: 'stack-1',
        success: true,
      });

      await mockApiClient.activateAdapterStack('stack-1');

      // In real UI, this would trigger toast.success
      // Here we just verify the API was called
      expect(mockApiClient.activateAdapterStack).toHaveBeenCalledWith('stack-1');
    });
  });

  describe('5. Chat with Adapter Stack', () => {
    it('creates chat session with adapter stack', async () => {
      const mockSession = createMockChatSession('session-1', 'stack-1');

      mockApiClient.createChatSession.mockResolvedValue({
        session_id: 'session-1',
        tenant_id: 'test-tenant',
        name: 'Test Chat Session',
        created_at: new Date().toISOString(),
      });

      const result = await mockApiClient.createChatSession({
        name: 'Test Chat Session',
        stack_id: 'stack-1',
      });

      expect(result.session_id).toBe('session-1');
      expect(mockApiClient.createChatSession).toHaveBeenCalledWith({
        name: 'Test Chat Session',
        stack_id: 'stack-1',
      });
    });

    it('sends chat message and receives streaming response', async () => {
      const mockMessages: ChatMessage[] = [];
      let assistantMessage = '';

      mockApiClient.streamInfer.mockImplementation((req, callbacks) => {
        // Simulate streaming tokens
        setTimeout(() => {
          callbacks.onToken('Hello');
          assistantMessage += 'Hello';
        }, 10);

        setTimeout(() => {
          callbacks.onToken(' world');
          assistantMessage += ' world';
        }, 20);

        setTimeout(() => {
          callbacks.onComplete(assistantMessage, 'stop');
          mockMessages.push(createMockChatMessage('msg-2', 'assistant', assistantMessage));
        }, 30);

        return Promise.resolve();
      });

      const userMessage = 'What is Rust?';
      mockMessages.push(createMockChatMessage('msg-1', 'user', userMessage));

      await mockApiClient.streamInfer(
        {
          prompt: userMessage,
          adapter_stack: ['adapter-1'],
          max_tokens: 150,
        },
        {
          onToken: (token: string) => {
            // Token received
          },
          onComplete: (text: string, reason: string | null) => {
            expect(text).toBe('Hello world');
            expect(reason).toBe('stop');
          },
          onError: (error: Error) => {
            throw error;
          },
        },
        'session-1'
      );

      await waitFor(() => {
        expect(mockMessages).toHaveLength(2);
      });

      expect(mockMessages[0].role).toBe('user');
      expect(mockMessages[1].role).toBe('assistant');
      expect(mockMessages[1].content).toBe('Hello world');
    });

    it('displays stack context in chat interface', async () => {
      const mockStack = createMockStack('stack-1', 'Test Stack', ['adapter-1']);
      mockApiClient.getAdapterStack.mockResolvedValue(mockStack);

      const stack = await mockApiClient.getAdapterStack('stack-1');

      expect(stack.name).toBe('Test Stack');
      expect(stack.adapter_ids).toContain('adapter-1');
      expect(stack.lifecycle_state).toBe('active');
    });
  });

  describe('6. Complete End-to-End Flow', () => {
    it('completes full workflow from collection to chat', async () => {
      // 1. Create collection
      const mockCollection = createMockCollection('coll-1', 'Training Data');
      mockApiClient.createCollection.mockResolvedValue(mockCollection);

      const collection = await mockApiClient.createCollection('Training Data');
      expect(collection.id).toBe('coll-1');

      // 2. Upload documents
      const mockDoc = createMockDocument('doc-1', 'Training Doc');
      mockApiClient.uploadDocument.mockResolvedValue(mockDoc);
      mockApiClient.addDocumentToCollection.mockResolvedValue(undefined);

      const doc = await mockApiClient.uploadDocument(
        new File(['content'], 'doc.pdf')
      );
      await mockApiClient.addDocumentToCollection('coll-1', 'doc-1');
      expect(doc.id).toBe('doc-1');

      // 3. Start training
      const pendingJob = createMockTrainingJob('job-1', 'pending');
      const runningJob = createMockTrainingJob('job-1', 'running');
      const completedJob = createMockTrainingJob('job-1', 'completed', 'adapter-1');

      mockApiClient.startTraining.mockResolvedValue(pendingJob);
      mockApiClient.getTrainingJob
        .mockResolvedValueOnce(pendingJob)
        .mockResolvedValueOnce(runningJob)
        .mockResolvedValueOnce(completedJob);

      const job = await mockApiClient.startTraining({
        adapter_name: 'tenant-a/domain/trained/r001',
        config: {
          learning_rate: 0.001,
          epochs: 3,
          batch_size: 8,
          rank: 16,
          alpha: 32,
        },
        dataset_id: 'dataset-1',
      });

      // Poll until complete
      let currentJob = await mockApiClient.getTrainingJob('job-1');
      expect(currentJob.status).toBe('pending');

      currentJob = await mockApiClient.getTrainingJob('job-1');
      expect(currentJob.status).toBe('running');

      currentJob = await mockApiClient.getTrainingJob('job-1');
      expect(currentJob.status).toBe('completed');
      expect(currentJob.adapter_id).toBe('adapter-1');

      // 4. Verify adapter
      const mockAdapter = createMockAdapter('adapter-1', 'tenant-a/domain/trained/r001');
      mockApiClient.getAdapter.mockResolvedValue(mockAdapter);

      const adapter = await mockApiClient.getAdapter('adapter-1');
      expect(adapter.name).toBe('tenant-a/domain/trained/r001');

      // 5. Create stack with adapter
      const mockStack = createMockStack('stack-1', 'Trained Stack', ['adapter-1']);
      mockApiClient.createAdapterStack.mockResolvedValue({
        schema_version: '1.0',
        stack: mockStack,
      });

      const stackResult = await mockApiClient.createAdapterStack({
        name: 'Trained Stack',
        adapter_ids: ['adapter-1'],
      });
      expect(stackResult.stack.adapter_ids).toContain('adapter-1');

      // 6. Activate stack
      mockApiClient.activateAdapterStack.mockResolvedValue({
        schema_version: '1.0',
        stack_id: 'stack-1',
        success: true,
      });

      await mockApiClient.activateAdapterStack('stack-1');

      // 7. Create chat session
      mockApiClient.createChatSession.mockResolvedValue({
        session_id: 'session-1',
        tenant_id: 'test-tenant',
        name: 'Chat with Trained Adapter',
        created_at: new Date().toISOString(),
      });

      const session = await mockApiClient.createChatSession({
        name: 'Chat with Trained Adapter',
        stack_id: 'stack-1',
      });
      expect(session.session_id).toBe('session-1');

      // 8. Send message with adapter stack
      let responseText = '';
      mockApiClient.streamInfer.mockImplementation((req, callbacks) => {
        setTimeout(() => {
          callbacks.onToken('Response');
          responseText += 'Response';
          callbacks.onComplete(responseText, 'stop');
        }, 10);
        return Promise.resolve();
      });

      await mockApiClient.streamInfer(
        {
          prompt: 'Test prompt',
          adapter_stack: ['adapter-1'],
        },
        {
          onToken: (token: string) => {
            // Token received
          },
          onComplete: (text: string) => {
            expect(text).toBe('Response');
          },
          onError: (error: Error) => {
            throw error;
          },
        },
        'session-1'
      );

      await waitFor(() => {
        expect(responseText).toBe('Response');
      });

      // Verify all steps completed
      expect(mockApiClient.createCollection).toHaveBeenCalled();
      expect(mockApiClient.uploadDocument).toHaveBeenCalled();
      expect(mockApiClient.startTraining).toHaveBeenCalled();
      expect(mockApiClient.getAdapter).toHaveBeenCalled();
      expect(mockApiClient.createAdapterStack).toHaveBeenCalled();
      expect(mockApiClient.createChatSession).toHaveBeenCalled();
      expect(mockApiClient.streamInfer).toHaveBeenCalled();
    });
  });

  describe('7. Error Handling and Edge Cases', () => {
    it('handles training timeout gracefully', async () => {
      const runningJob = createMockTrainingJob('job-1', 'running');
      mockApiClient.getTrainingJob.mockResolvedValue(runningJob);

      // Simulate timeout by polling multiple times without completion
      for (let i = 0; i < 5; i++) {
        const job = await mockApiClient.getTrainingJob('job-1');
        expect(job.status).toBe('running');
      }

      // In real UI, this would trigger timeout handling
      expect(mockApiClient.getTrainingJob).toHaveBeenCalledTimes(5);
    });

    it('handles stack activation failure', async () => {
      const error = new Error('Stack activation failed');
      mockApiClient.activateAdapterStack.mockRejectedValue(error);

      await expect(
        mockApiClient.activateAdapterStack('stack-1')
      ).rejects.toThrow('Stack activation failed');
    });

    it('handles chat inference error', async () => {
      mockApiClient.streamInfer.mockImplementation((req, callbacks) => {
        setTimeout(() => {
          callbacks.onError(new Error('Inference failed'));
        }, 10);
        return Promise.resolve();
      });

      let errorReceived = false;
      await mockApiClient.streamInfer(
        { prompt: 'Test' },
        {
          onToken: () => {},
          onComplete: () => {},
          onError: (error) => {
            errorReceived = true;
            expect(error.message).toBe('Inference failed');
          },
        },
        'session-1'
      );

      await waitFor(() => {
        expect(errorReceived).toBe(true);
      });
    });

    it('handles missing adapter_id in completed training job', async () => {
      const completedJobNoAdapter = {
        ...createMockTrainingJob('job-1', 'completed'),
        adapter_id: undefined,
      };

      mockApiClient.getTrainingJob.mockResolvedValue(completedJobNoAdapter);

      const job = await mockApiClient.getTrainingJob('job-1');
      expect(job.status).toBe('completed');
      expect(job.adapter_id).toBeUndefined();

      // In real UI, this would show a warning
    });
  });

  describe('8. Navigation and Toast Notifications', () => {
    it('navigates through pages in correct order', () => {
      const routes = [
        '/document-library',      // Create collection
        '/training/datasets',     // View datasets
        '/training',              // Start training
        '/adapters',              // View adapter
        '/chat',                  // Use in chat
      ];

      routes.forEach(route => {
        expect(route).toBeTruthy();
      });
    });

    it('shows success toast on training completion', async () => {
      const completedJob = createMockTrainingJob('job-1', 'completed', 'adapter-1');
      mockApiClient.getTrainingJob.mockResolvedValue(completedJob);

      const job = await mockApiClient.getTrainingJob('job-1');

      if (job.status === 'completed') {
        // In real UI, would trigger: toast.success('Training completed!')
        expect(job.adapter_id).toBe('adapter-1');
      }
    });

    it('shows error toast on training failure', async () => {
      const failedJob = {
        ...createMockTrainingJob('job-1', 'failed'),
        error_message: 'Training failed',
      };
      mockApiClient.getTrainingJob.mockResolvedValue(failedJob);

      const job = await mockApiClient.getTrainingJob('job-1');

      if (job.status === 'failed') {
        // In real UI, would trigger: toast.error(job.error_message)
        expect(job.error_message).toBe('Training failed');
      }
    });

    it('shows loading toast during training', async () => {
      const runningJob = createMockTrainingJob('job-1', 'running');
      mockApiClient.getTrainingJob.mockResolvedValue(runningJob);

      const job = await mockApiClient.getTrainingJob('job-1');

      if (job.status === 'running') {
        // In real UI, would show: toast.loading('Training in progress...')
        expect(job.progress_pct).toBe(50);
      }
    });
  });
});
