/**
 * Mock factory functions for testing API hooks and components
 *
 * Provides factory functions for creating consistent mock objects across tests.
 */

import type {
  Document,
  Collection,
  DocumentMetadata,
  CollectionMetadata,
  Evidence,
  ChatSession,
  PolicyCheck,
  PolicyCheckResult,
} from '@/api/chat-types';

/**
 * Creates a mock Document object with default or custom values
 */
export function createMockDocument(overrides?: Partial<Document>): Document {
  return {
    id: overrides?.id ?? 'doc-1',
    collection_id: overrides?.collection_id ?? 'collection-1',
    content: overrides?.content ?? 'Sample document content',
    metadata: overrides?.metadata ?? createMockDocumentMetadata(),
    embedding_model: overrides?.embedding_model ?? 'text-embedding-3-small',
    created_at: overrides?.created_at ?? '2025-01-15T10:00:00Z',
    updated_at: overrides?.updated_at ?? '2025-01-15T10:00:00Z',
  };
}

/**
 * Creates mock DocumentMetadata with default or custom values
 */
export function createMockDocumentMetadata(
  overrides?: Partial<DocumentMetadata>
): DocumentMetadata {
  return {
    title: overrides?.title ?? 'Sample Document',
    source: overrides?.source ?? 'manual-upload',
    filename: overrides?.filename ?? 'sample.txt',
    mime_type: overrides?.mime_type ?? 'text/plain',
    size_bytes: overrides?.size_bytes ?? 1024,
    chunk_index: overrides?.chunk_index,
    total_chunks: overrides?.total_chunks,
    tags: overrides?.tags ?? ['test', 'sample'],
    ...overrides,
  };
}

/**
 * Creates a mock Collection object with default or custom values
 */
export function createMockCollection(overrides?: Partial<Collection>): Collection {
  return {
    id: overrides?.id ?? 'collection-1',
    name: overrides?.name ?? 'Test Collection',
    description: overrides?.description ?? 'A test collection',
    tenant_id: overrides?.tenant_id ?? 'tenant-1',
    metadata: overrides?.metadata ?? createMockCollectionMetadata(),
    document_count: overrides?.document_count ?? 5,
    created_at: overrides?.created_at ?? '2025-01-15T09:00:00Z',
    updated_at: overrides?.updated_at ?? '2025-01-15T09:00:00Z',
  };
}

/**
 * Creates mock CollectionMetadata with default or custom values
 */
export function createMockCollectionMetadata(
  overrides?: Partial<CollectionMetadata>
): CollectionMetadata {
  return {
    category: overrides?.category ?? 'general',
    tags: overrides?.tags ?? ['test'],
    owner: overrides?.owner ?? 'test-user',
    visibility: overrides?.visibility ?? 'private',
    ...overrides,
  };
}

/**
 * Creates a mock Evidence object with default or custom values
 */
export function createMockEvidence(overrides?: Partial<Evidence>): Evidence {
  return {
    id: overrides?.id ?? 'evidence-1',
    message_id: overrides?.message_id ?? 'msg-1',
    document_id: overrides?.document_id ?? 'doc-1',
    collection_id: overrides?.collection_id ?? 'collection-1',
    relevance_score: overrides?.relevance_score ?? 0.85,
    confidence_score: overrides?.confidence_score ?? 0.92,
    snippet: overrides?.snippet ?? 'Relevant document snippet...',
    document_title: overrides?.document_title ?? 'Sample Document',
    collection_name: overrides?.collection_name ?? 'Test Collection',
    created_at: overrides?.created_at ?? '2025-01-15T10:05:00Z',
  };
}

/**
 * Creates a mock ChatSession object with default or custom values
 */
export function createMockChatSession(overrides?: Partial<ChatSession>): ChatSession {
  return {
    id: overrides?.id ?? 'session-1',
    tenant_id: overrides?.tenant_id ?? 'tenant-1',
    title: overrides?.title ?? 'Test Chat Session',
    adapter_stack_id: overrides?.adapter_stack_id ?? 'stack-1',
    collection_id: overrides?.collection_id ?? 'collection-1',
    collection_name: overrides?.collection_name ?? 'Test Collection',
    created_at: overrides?.created_at ?? '2025-01-15T09:00:00Z',
    updated_at: overrides?.updated_at ?? '2025-01-15T10:00:00Z',
    last_message_at: overrides?.last_message_at ?? '2025-01-15T10:05:00Z',
    message_count: overrides?.message_count ?? 3,
    ...overrides,
  };
}

/**
 * Creates a mock PolicyCheck object with default or custom values
 */
export function createMockPolicyCheck(overrides?: Partial<PolicyCheck>): PolicyCheck {
  return {
    policy_id: overrides?.policy_id ?? 'policy-1',
    policy_name: overrides?.policy_name ?? 'Evidence Quality',
    check_type: overrides?.check_type ?? 'evidence_threshold',
    status: overrides?.status ?? 'pass',
    details: overrides?.details ?? 'All checks passed',
    timestamp: overrides?.timestamp ?? '2025-01-15T10:05:00Z',
  };
}

/**
 * Creates a mock PolicyCheckResult with default or custom values
 */
export function createMockPolicyCheckResult(
  overrides?: Partial<PolicyCheckResult>
): PolicyCheckResult {
  return {
    message_id: overrides?.message_id ?? 'msg-1',
    checks: overrides?.checks ?? [
      createMockPolicyCheck(),
      createMockPolicyCheck({
        policy_id: 'policy-2',
        policy_name: 'Source Validation',
        check_type: 'source_verification',
      }),
    ],
    overall_status: overrides?.overall_status ?? 'pass',
    created_at: overrides?.created_at ?? '2025-01-15T10:05:00Z',
  };
}

/**
 * Creates a batch of mock Documents for testing lists
 */
export function createMockDocumentList(count: number = 5): Document[] {
  return Array.from({ length: count }, (_, i) =>
    createMockDocument({
      id: `doc-${i + 1}`,
      metadata: createMockDocumentMetadata({
        title: `Document ${i + 1}`,
      }),
    })
  );
}

/**
 * Creates a batch of mock Collections for testing lists
 */
export function createMockCollectionList(count: number = 3): Collection[] {
  return Array.from({ length: count }, (_, i) =>
    createMockCollection({
      id: `collection-${i + 1}`,
      name: `Collection ${i + 1}`,
      document_count: (i + 1) * 5,
    })
  );
}

/**
 * Creates a batch of mock Evidence for testing lists
 */
export function createMockEvidenceList(count: number = 3): Evidence[] {
  return Array.from({ length: count }, (_, i) =>
    createMockEvidence({
      id: `evidence-${i + 1}`,
      document_id: `doc-${i + 1}`,
      relevance_score: 0.9 - i * 0.1,
      snippet: `Evidence snippet ${i + 1}...`,
    })
  );
}

/**
 * Creates a batch of mock ChatSessions for testing lists
 */
export function createMockChatSessionList(count: number = 3): ChatSession[] {
  return Array.from({ length: count }, (_, i) =>
    createMockChatSession({
      id: `session-${i + 1}`,
      title: `Chat Session ${i + 1}`,
      message_count: (i + 1) * 3,
    })
  );
}

/**
 * Utility to create a mock error response
 */
export function createMockError(message: string = 'Mock error', code?: string) {
  return {
    error: message,
    code: code ?? 'MOCK_ERROR',
    details: {},
  };
}

/**
 * Utility to create a mock success response with pagination.
 * Matches PaginatedResponse<T> structure from backend.
 */
export function createMockPaginatedResponse<T>(
  data: T[],
  total?: number,
  page: number = 1,
  limit: number = 20
) {
  const actualTotal = total ?? data.length;
  return {
    schema_version: '1.0',
    data,
    total: actualTotal,
    page,
    limit,
    pages: Math.ceil(actualTotal / limit),
  };
}
