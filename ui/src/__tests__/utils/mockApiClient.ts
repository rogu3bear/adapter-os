/**
 * Mock API client implementations for testing
 *
 * Provides mock implementations of all API methods with configurable responses.
 */

import type {
  Document,
  Collection,
  Evidence,
  ChatSession,
  PolicyCheckResult,
  CreateDocumentRequest,
  UpdateDocumentRequest,
  CreateCollectionRequest,
  UpdateCollectionRequest,
  CreateChatSessionRequest,
  UpdateChatSessionRequest,
  DocumentSearchParams,
  CollectionSearchParams,
} from '@/api/chat-types';

import {
  createMockDocument,
  createMockCollection,
  createMockEvidence,
  createMockChatSession,
  createMockPolicyCheckResult,
  createMockDocumentList,
  createMockCollectionList,
  createMockEvidenceList,
  createMockChatSessionList,
  createMockPaginatedResponse,
  createMockError,
} from './mockFactories';

/**
 * Configuration for mock API responses
 */
export interface MockApiConfig {
  // Response delays (in ms)
  delay?: number;
  // Whether to simulate errors
  shouldError?: boolean;
  // Custom error message
  errorMessage?: string;
  // Custom error code
  errorCode?: string;
}

/**
 * Mock state for tracking API calls
 */
export class MockApiState {
  private documents: Map<string, Document> = new Map();
  private collections: Map<string, Collection> = new Map();
  private evidence: Map<string, Evidence[]> = new Map();
  private chatSessions: Map<string, ChatSession> = new Map();
  private policyChecks: Map<string, PolicyCheckResult> = new Map();

  constructor() {
    // Initialize with default mock data
    this.reset();
  }

  reset() {
    this.documents.clear();
    this.collections.clear();
    this.evidence.clear();
    this.chatSessions.clear();
    this.policyChecks.clear();

    // Add default data
    createMockDocumentList(5).forEach((doc) => this.documents.set(doc.id, doc));
    createMockCollectionList(3).forEach((col) => this.collections.set(col.id, col));
    createMockChatSessionList(3).forEach((session) =>
      this.chatSessions.set(session.id, session)
    );
  }

  // Documents
  getDocument(id: string): Document | undefined {
    return this.documents.get(id);
  }

  getDocuments(): Document[] {
    return Array.from(this.documents.values());
  }

  addDocument(doc: Document) {
    this.documents.set(doc.id, doc);
  }

  updateDocument(id: string, updates: Partial<Document>) {
    const doc = this.documents.get(id);
    if (doc) {
      this.documents.set(id, { ...doc, ...updates });
    }
  }

  deleteDocument(id: string) {
    this.documents.delete(id);
  }

  // Collections
  getCollection(id: string): Collection | undefined {
    return this.collections.get(id);
  }

  getCollections(): Collection[] {
    return Array.from(this.collections.values());
  }

  addCollection(col: Collection) {
    this.collections.set(col.id, col);
  }

  updateCollection(id: string, updates: Partial<Collection>) {
    const col = this.collections.get(id);
    if (col) {
      this.collections.set(id, { ...col, ...updates });
    }
  }

  deleteCollection(id: string) {
    this.collections.delete(id);
  }

  // Evidence
  getEvidence(messageId: string): Evidence[] {
    return this.evidence.get(messageId) ?? [];
  }

  setEvidence(messageId: string, evidenceList: Evidence[]) {
    this.evidence.set(messageId, evidenceList);
  }

  // Chat Sessions
  getChatSession(id: string): ChatSession | undefined {
    return this.chatSessions.get(id);
  }

  getChatSessions(): ChatSession[] {
    return Array.from(this.chatSessions.values());
  }

  addChatSession(session: ChatSession) {
    this.chatSessions.set(session.id, session);
  }

  updateChatSession(id: string, updates: Partial<ChatSession>) {
    const session = this.chatSessions.get(id);
    if (session) {
      this.chatSessions.set(id, { ...session, ...updates });
    }
  }

  deleteChatSession(id: string) {
    this.chatSessions.delete(id);
  }

  // Policy Checks
  getPolicyCheck(messageId: string): PolicyCheckResult | undefined {
    return this.policyChecks.get(messageId);
  }

  setPolicyCheck(messageId: string, result: PolicyCheckResult) {
    this.policyChecks.set(messageId, result);
  }
}

/**
 * Mock API client with configurable responses
 */
export class MockApiClient {
  private state: MockApiState;
  private config: MockApiConfig;

  constructor(config: MockApiConfig = {}) {
    this.state = new MockApiState();
    this.config = config;
  }

  private async simulateDelay() {
    if (this.config.delay) {
      await new Promise((resolve) => setTimeout(resolve, this.config.delay));
    }
  }

  private throwIfError() {
    if (this.config.shouldError) {
      throw createMockError(
        this.config.errorMessage ?? 'Mock API error',
        this.config.errorCode
      );
    }
  }

  // Reset state
  reset() {
    this.state.reset();
  }

  // Update config
  setConfig(config: Partial<MockApiConfig>) {
    this.config = { ...this.config, ...config };
  }

  // Documents API
  async getDocuments(params?: DocumentSearchParams) {
    await this.simulateDelay();
    this.throwIfError();

    let documents = this.state.getDocuments();

    // Apply filters
    if (params?.collection_id) {
      documents = documents.filter((d) => d.collection_id === params.collection_id);
    }
    if (params?.search) {
      const search = params.search.toLowerCase();
      documents = documents.filter(
        (d) =>
          d.content.toLowerCase().includes(search) ||
          d.metadata.title?.toLowerCase().includes(search)
      );
    }
    if (params?.tags) {
      documents = documents.filter((d) =>
        params.tags!.some((tag) => d.metadata.tags?.includes(tag))
      );
    }

    // Pagination
    const limit = params?.limit ?? 20;
    const offset = params?.offset ?? 0;
    const paginatedDocs = documents.slice(offset, offset + limit);

    return createMockPaginatedResponse(paginatedDocs, documents.length, 1, limit);
  }

  async getDocument(id: string) {
    await this.simulateDelay();
    this.throwIfError();

    const doc = this.state.getDocument(id);
    if (!doc) {
      throw createMockError(`Document ${id} not found`, 'NOT_FOUND');
    }
    return doc;
  }

  async createDocument(req: CreateDocumentRequest) {
    await this.simulateDelay();
    this.throwIfError();

    const doc = createMockDocument({
      id: `doc-${Date.now()}`,
      collection_id: req.collection_id,
      content: req.content,
      metadata: req.metadata,
      embedding_model: req.embedding_model,
    });

    this.state.addDocument(doc);
    return doc;
  }

  async updateDocument(id: string, req: UpdateDocumentRequest) {
    await this.simulateDelay();
    this.throwIfError();

    const doc = this.state.getDocument(id);
    if (!doc) {
      throw createMockError(`Document ${id} not found`, 'NOT_FOUND');
    }

    this.state.updateDocument(id, {
      ...req,
      updated_at: new Date().toISOString(),
    });

    return this.state.getDocument(id)!;
  }

  async deleteDocument(id: string) {
    await this.simulateDelay();
    this.throwIfError();

    const doc = this.state.getDocument(id);
    if (!doc) {
      throw createMockError(`Document ${id} not found`, 'NOT_FOUND');
    }

    this.state.deleteDocument(id);
  }

  // Collections API
  async getCollections(params?: CollectionSearchParams) {
    await this.simulateDelay();
    this.throwIfError();

    let collections = this.state.getCollections();

    // Apply filters
    if (params?.search) {
      const search = params.search.toLowerCase();
      collections = collections.filter(
        (c) =>
          c.name.toLowerCase().includes(search) ||
          c.description?.toLowerCase().includes(search)
      );
    }

    // Pagination
    const limit = params?.limit ?? 20;
    const offset = params?.offset ?? 0;
    const paginatedCols = collections.slice(offset, offset + limit);

    return createMockPaginatedResponse(paginatedCols, collections.length, 1, limit);
  }

  async getCollection(id: string) {
    await this.simulateDelay();
    this.throwIfError();

    const col = this.state.getCollection(id);
    if (!col) {
      throw createMockError(`Collection ${id} not found`, 'NOT_FOUND');
    }
    return col;
  }

  async createCollection(req: CreateCollectionRequest) {
    await this.simulateDelay();
    this.throwIfError();

    const col = createMockCollection({
      id: `collection-${Date.now()}`,
      name: req.name,
      description: req.description,
      tenant_id: req.tenant_id,
      metadata: req.metadata,
      document_count: 0,
    });

    this.state.addCollection(col);
    return col;
  }

  async updateCollection(id: string, req: UpdateCollectionRequest) {
    await this.simulateDelay();
    this.throwIfError();

    const col = this.state.getCollection(id);
    if (!col) {
      throw createMockError(`Collection ${id} not found`, 'NOT_FOUND');
    }

    this.state.updateCollection(id, {
      ...req,
      updated_at: new Date().toISOString(),
    });

    return this.state.getCollection(id)!;
  }

  async deleteCollection(id: string) {
    await this.simulateDelay();
    this.throwIfError();

    const col = this.state.getCollection(id);
    if (!col) {
      throw createMockError(`Collection ${id} not found`, 'NOT_FOUND');
    }

    this.state.deleteCollection(id);
  }

  // Evidence API
  async getEvidence(messageId: string) {
    await this.simulateDelay();
    this.throwIfError();

    let evidence = this.state.getEvidence(messageId);

    // Generate default evidence if none exists
    if (evidence.length === 0) {
      evidence = createMockEvidenceList(3).map((e) => ({ ...e, message_id: messageId }));
      this.state.setEvidence(messageId, evidence);
    }

    return evidence;
  }

  // Chat Sessions API
  async getChatSessions(tenantId: string) {
    await this.simulateDelay();
    this.throwIfError();

    const sessions = this.state
      .getChatSessions()
      .filter((s) => s.tenant_id === tenantId);

    return sessions;
  }

  async getChatSession(id: string) {
    await this.simulateDelay();
    this.throwIfError();

    const session = this.state.getChatSession(id);
    if (!session) {
      throw createMockError(`Chat session ${id} not found`, 'NOT_FOUND');
    }
    return session;
  }

  async createChatSession(req: CreateChatSessionRequest) {
    await this.simulateDelay();
    this.throwIfError();

    const session = createMockChatSession({
      id: `session-${Date.now()}`,
      tenant_id: req.tenant_id,
      title: req.title,
      adapter_stack_id: req.adapter_stack_id,
      collection_id: req.collection_id,
      message_count: 0,
    });

    this.state.addChatSession(session);
    return session;
  }

  async updateChatSession(id: string, req: UpdateChatSessionRequest) {
    await this.simulateDelay();
    this.throwIfError();

    const session = this.state.getChatSession(id);
    if (!session) {
      throw createMockError(`Chat session ${id} not found`, 'NOT_FOUND');
    }

    this.state.updateChatSession(id, {
      ...req,
      updated_at: new Date().toISOString(),
    });

    return this.state.getChatSession(id)!;
  }

  async deleteChatSession(id: string) {
    await this.simulateDelay();
    this.throwIfError();

    const session = this.state.getChatSession(id);
    if (!session) {
      throw createMockError(`Chat session ${id} not found`, 'NOT_FOUND');
    }

    this.state.deleteChatSession(id);
  }

  // Policy Check API
  async checkMessagePolicy(messageId: string) {
    await this.simulateDelay();
    this.throwIfError();

    let result = this.state.getPolicyCheck(messageId);

    // Generate default policy check if none exists
    if (!result) {
      result = createMockPolicyCheckResult({ message_id: messageId });
      this.state.setPolicyCheck(messageId, result);
    }

    return result;
  }

  // State access for testing
  getState(): MockApiState {
    return this.state;
  }
}

/**
 * Create a new mock API client instance
 */
export function createMockApiClient(config?: MockApiConfig): MockApiClient {
  return new MockApiClient(config);
}

/**
 * Setup mock API responses for MSW or similar
 */
export function setupMockApiResponses(client: MockApiClient) {
  return {
    // Documents
    'GET /v1/documents': () => client.getDocuments(),
    'GET /v1/documents/:id': ({ params }: { params: { id: string } }) =>
      client.getDocument(params.id),
    'POST /v1/documents': ({ body }: { body: CreateDocumentRequest }) =>
      client.createDocument(body),
    'PUT /v1/documents/:id': ({
      params,
      body,
    }: {
      params: { id: string };
      body: UpdateDocumentRequest;
    }) => client.updateDocument(params.id, body),
    'DELETE /v1/documents/:id': ({ params }: { params: { id: string } }) =>
      client.deleteDocument(params.id),

    // Collections
    'GET /v1/collections': () => client.getCollections(),
    'GET /v1/collections/:id': ({ params }: { params: { id: string } }) =>
      client.getCollection(params.id),
    'POST /v1/collections': ({ body }: { body: CreateCollectionRequest }) =>
      client.createCollection(body),
    'PUT /v1/collections/:id': ({
      params,
      body,
    }: {
      params: { id: string };
      body: UpdateCollectionRequest;
    }) => client.updateCollection(params.id, body),
    'DELETE /v1/collections/:id': ({ params }: { params: { id: string } }) =>
      client.deleteCollection(params.id),

    // Evidence
    'GET /v1/evidence/:messageId': ({ params }: { params: { messageId: string } }) =>
      client.getEvidence(params.messageId),

    // Chat Sessions
    'GET /v1/chat-sessions': ({ query }: { query: { tenant_id: string } }) =>
      client.getChatSessions(query.tenant_id),
    'GET /v1/chat-sessions/:id': ({ params }: { params: { id: string } }) =>
      client.getChatSession(params.id),
    'POST /v1/chat-sessions': ({ body }: { body: CreateChatSessionRequest }) =>
      client.createChatSession(body),
    'PUT /v1/chat-sessions/:id': ({
      params,
      body,
    }: {
      params: { id: string };
      body: UpdateChatSessionRequest;
    }) => client.updateChatSession(params.id, body),
    'DELETE /v1/chat-sessions/:id': ({ params }: { params: { id: string } }) =>
      client.deleteChatSession(params.id),

    // Policy Checks
    'GET /v1/policy-checks/:messageId': ({
      params,
    }: {
      params: { messageId: string };
    }) => client.checkMessagePolicy(params.messageId),
  };
}
