/**
 * Chat service - handles chat sessions, messages, tags, categories, sharing, and owner chat.
 *
 * This service uses the transformer pattern to convert between:
 * - Backend snake_case responses → Frontend camelCase domain types
 * - Frontend camelCase requests → Backend snake_case requests
 *
 * All return types use camelCase fields for consistent frontend usage.
 */

import type { ApiClient } from '@/api/client';
import * as chatTypes from '@/api/chat-types';
import * as ownerTypes from '@/api/owner-types';
import * as authTypes from '@/api/auth-types';
import { CamelCaseKeys, toCamelCase, toSnakeCase } from '@/api/transformers';
import { logger } from '@/utils/logger';

// Domain types (camelCase versions of backend types)
// These types represent the frontend-facing data after toCamelCase() transformation.
// Use CamelCaseKeys<T> to transform snake_case backend types to camelCase frontend types.
// This provides type-safe property access with camelCase naming in components and hooks.
export type ChatSession = CamelCaseKeys<chatTypes.ChatSession>;
export type ChatMessage = CamelCaseKeys<chatTypes.ChatMessage>;
export type ChatSessionTrace = CamelCaseKeys<chatTypes.ChatSessionTrace>;
export type ChatEvidenceItem = CamelCaseKeys<chatTypes.ChatEvidenceItem>;
export type ChatTag = CamelCaseKeys<chatTypes.ChatTag>;
export type ChatCategory = CamelCaseKeys<chatTypes.ChatCategory>;
export type ChatSearchResult = CamelCaseKeys<chatTypes.ChatSearchResult>;
export type ChatSessionWithStatus = CamelCaseKeys<chatTypes.ChatSessionWithStatus>;
export type SessionShare = CamelCaseKeys<chatTypes.SessionShare>;
export type SessionSummary = CamelCaseKeys<chatTypes.SessionSummary>;
export type CreateChatSessionResponse = CamelCaseKeys<chatTypes.CreateChatSessionResponse>;
export type ShareSessionResponse = CamelCaseKeys<chatTypes.ShareSessionResponse>;

export class ChatService {
  constructor(private client: ApiClient) {}

  // ============================================================================
  // Chat Session Management
  // ============================================================================

  /**
   * Create a new chat session
   *
   * POST /v1/chat/sessions
   *
   * @param req - Session creation request
   * @returns Created session response with camelCase fields
   */
  async createChatSession(req: chatTypes.CreateChatSessionRequest): Promise<CreateChatSessionResponse> {
    logger.info('Creating chat session', {
      component: 'ChatService',
      operation: 'createChatSession',
      name: req.name,
      stack_id: req.stack_id,
      collection_id: req.collection_id,
      document_id: req.document_id,
      source_type: req.source_type,
    });

    // Build metadata, including document context when provided (backend stores in metadata_json)
    const metadata = {
      ...(req.metadata || {}),
      ...(req.source_type ? { source_type: req.source_type } : {}),
      ...(req.document_id ? { documentId: req.document_id } : {}),
      ...(req.document_name ? { documentName: req.document_name } : {}),
    };

    // Convert metadata object to JSON string if present
    const payload = {
      name: req.name,
      title: req.title,
      tenant_id: req.tenant_id,
      stack_id: req.stack_id,
      collection_id: req.collection_id,
      document_id: req.document_id,
      document_name: req.document_name,
      source_type: req.source_type,
      source_ref_id: req.source_ref_id,
      metadata_json: Object.keys(metadata).length > 0 ? JSON.stringify(metadata) : undefined,
    };

    const response = await this.client.request<chatTypes.CreateChatSessionResponse>('/v1/chat/sessions', {
      method: 'POST',
      body: JSON.stringify(payload),
    });

    return toCamelCase(response);
  }

  /**
   * Update an existing chat session
   *
   * PUT /v1/chat/sessions/:session_id
   *
   * @param sessionId - Session ID
   * @param req - Update request (snake_case fields)
   * @returns Updated chat session with camelCase fields
   */
  async updateChatSession(
    sessionId: string,
    req: chatTypes.UpdateChatSessionRequest
  ): Promise<ChatSession> {
    logger.info('Updating chat session', {
      component: 'ChatService',
      operation: 'updateChatSession',
      sessionId,
      stack_id: req.stack_id,
      collection_id: req.collection_id,
      document_id: req.document_id,
      source_type: req.source_type,
    });

    const response = await this.client.request<chatTypes.ChatSession>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}`,
      {
        method: 'PUT',
        body: JSON.stringify(req),
      }
    );

    return toCamelCase<chatTypes.ChatSession>(response);
  }

  /**
   * List chat sessions for current user/tenant
   *
   * GET /v1/chat/sessions
   *
   * @param query - Optional filters
   * @returns Array of chat sessions with camelCase fields
   */
  async listChatSessions(query?: chatTypes.ListSessionsQuery): Promise<ChatSession[]> {
    const params = new URLSearchParams();
    if (query?.user_id) params.append('user_id', query.user_id);
    if (query?.limit) params.append('limit', query.limit.toString());
    if (query?.source_type) params.append('source_type', query.source_type);
    if (query?.document_id) params.append('document_id', query.document_id);

    const queryString = params.toString();
    const response = await this.client.requestList<chatTypes.ChatSession>(
      `/v1/chat/sessions${queryString ? `?${queryString}` : ''}`
    );

    return toCamelCase<chatTypes.ChatSession[]>(response);
  }

  /**
   * Get a specific chat session
   *
   * GET /v1/chat/sessions/:session_id
   *
   * @param sessionId - Session ID
   * @returns Chat session with camelCase fields
   */
  async getChatSession(sessionId: string): Promise<ChatSession> {
    const response = await this.client.request<chatTypes.ChatSession>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}`
    );

    return toCamelCase<chatTypes.ChatSession>(response);
  }

  /**
   * Delete a chat session
   *
   * DELETE /v1/chat/sessions/:session_id
   *
   * @param sessionId - Session ID
   */
  async deleteChatSession(sessionId: string): Promise<void> {
    logger.info('Deleting chat session', {
      component: 'ChatService',
      operation: 'deleteChatSession',
      sessionId,
    });
    return this.client.request<void>(`/v1/chat/sessions/${encodeURIComponent(sessionId)}`, {
      method: 'DELETE',
    });
  }

  // ============================================================================
  // Chat Message Management
  // ============================================================================

  /**
   * Add a message to a chat session
   *
   * POST /v1/chat/sessions/:session_id/messages
   *
   * @param sessionId - Session ID
   * @param role - Message role (user, assistant, system)
   * @param content - Message content
   * @param metadata - Optional metadata
   * @returns Created message with camelCase fields
   */
  async addChatMessage(
    sessionId: string,
    role: 'user' | 'assistant' | 'system',
    content: string,
    metadata?: Record<string, unknown>
  ): Promise<ChatMessage> {
    const payload: chatTypes.AddChatMessageRequest = {
      role,
      content,
      metadata,
    };

    // Convert metadata object to JSON string if present
    const requestBody = {
      role,
      content,
      metadata_json: metadata ? JSON.stringify(metadata) : undefined,
    };

    const response = await this.client.request<chatTypes.ChatMessage>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
      {
        method: 'POST',
        body: JSON.stringify(requestBody),
      }
    );

    return toCamelCase<chatTypes.ChatMessage>(response);
  }

  /**
   * Get messages for a chat session
   *
   * GET /v1/chat/sessions/:session_id/messages
   *
   * @param sessionId - Session ID
   * @param limit - Optional limit on number of messages
   * @returns Array of chat messages with camelCase fields
   */
  async getChatMessages(sessionId: string, limit?: number): Promise<ChatMessage[]> {
    const params = new URLSearchParams();
    if (limit) params.append('limit', limit.toString());

    const queryString = params.toString();
    const response = await this.client.requestList<chatTypes.ChatMessage>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages${queryString ? `?${queryString}` : ''}`
    );

    return toCamelCase<chatTypes.ChatMessage[]>(response);
  }

  /**
   * Get evidence attached to a chat message
   *
   * GET /v1/chat/messages/:message_id/evidence
   *
   * @param messageId - Message ID
   * @returns Evidence items for the message with camelCase fields
   */
  async getMessageEvidence(messageId: string): Promise<ChatEvidenceItem[]> {
    const response = await this.client.requestList<chatTypes.ChatEvidenceItem>(
      `/v1/chat/messages/${encodeURIComponent(messageId)}/evidence`
    );

    return toCamelCase<chatTypes.ChatEvidenceItem[]>(response);
  }

  // ============================================================================
  // Session Utilities
  // ============================================================================

  /**
   * Get session summary with message and trace counts
   *
   * GET /v1/chat/sessions/:session_id/summary
   *
   * @param sessionId - Session ID
   * @returns Session summary with camelCase fields
   */
  async getSessionSummary(sessionId: string): Promise<SessionSummary> {
    const response = await this.client.request<chatTypes.SessionSummary>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/summary`
    );

    return toCamelCase<chatTypes.SessionSummary>(response);
  }

  /**
   * Update session collection binding
   *
   * PUT /v1/chat/sessions/:session_id/collection
   *
   * @param sessionId - Session ID
   * @param collectionId - Collection ID (or null to clear)
   */
  async updateSessionCollection(sessionId: string, collectionId: string | null): Promise<void> {
    logger.info('Updating session collection', {
      component: 'ChatService',
      operation: 'updateSessionCollection',
      sessionId,
      collectionId,
    });

    const payload: chatTypes.UpdateSessionCollectionRequest = {
      collection_id: collectionId,
    };

    return this.client.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/collection`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      }
    );
  }

  // ============================================================================
  // Session Lifecycle (Archive/Restore/Delete)
  // ============================================================================

  /**
   * Archive a chat session
   *
   * POST /v1/chat/sessions/:session_id/archive
   *
   * @param sessionId - Session ID
   * @param reason - Optional reason for archiving
   */
  async archiveChatSession(sessionId: string, reason?: string): Promise<void> {
    logger.info('Archiving chat session', {
      component: 'ChatService',
      operation: 'archiveChatSession',
      sessionId,
    });

    const payload: chatTypes.ArchiveSessionRequest = {
      reason,
    };

    return this.client.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/archive`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );
  }

  /**
   * Restore a deleted or archived chat session (admin-only)
   *
   * POST /v1/chat/sessions/:session_id/restore
   *
   * @param sessionId - Session ID
   */
  async restoreChatSession(sessionId: string): Promise<void> {
    logger.info('Restoring chat session', {
      component: 'ChatService',
      operation: 'restoreChatSession',
      sessionId,
    });

    return this.client.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/restore`,
      {
        method: 'POST',
      }
    );
  }

  /**
   * Permanently delete a chat session (admin-only)
   *
   * DELETE /v1/chat/sessions/:session_id/hard
   *
   * @param sessionId - Session ID
   */
  async hardDeleteChatSession(sessionId: string): Promise<void> {
    logger.info('Hard deleting chat session', {
      component: 'ChatService',
      operation: 'hardDeleteChatSession',
      sessionId,
    });

    return this.client.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/permanent`,
      {
        method: 'DELETE',
      }
    );
  }

  /**
   * List archived chat sessions
   *
   * GET /v1/chat/sessions/archived
   *
   * @param limit - Optional limit on number of sessions
   * @returns Array of archived chat sessions with status (camelCase fields)
   */
  async listArchivedChatSessions(limit?: number): Promise<ChatSessionWithStatus[]> {
    const params = new URLSearchParams();
    if (limit) params.append('limit', limit.toString());

    const queryString = params.toString();
    const response = await this.client.requestList<chatTypes.ChatSessionWithStatus>(
      `/v1/chat/sessions/archived${queryString ? `?${queryString}` : ''}`
    );

    return toCamelCase<chatTypes.ChatSessionWithStatus[]>(response);
  }

  /**
   * List deleted chat sessions (trash)
   *
   * GET /v1/chat/sessions/trash
   *
   * @param limit - Optional limit on number of sessions
   * @returns Array of deleted chat sessions with status (camelCase fields)
   */
  async listDeletedChatSessions(limit?: number): Promise<ChatSessionWithStatus[]> {
    const params = new URLSearchParams();
    if (limit) params.append('limit', limit.toString());

    const queryString = params.toString();
    const response = await this.client.requestList<chatTypes.ChatSessionWithStatus>(
      `/v1/chat/sessions/trash${queryString ? `?${queryString}` : ''}`
    );

    return toCamelCase<chatTypes.ChatSessionWithStatus[]>(response);
  }

  // ============================================================================
  // Session Search
  // ============================================================================

  /**
   * Search chat sessions using FTS (Full-Text Search)
   *
   * GET /v1/chat/sessions/search
   *
   * @param query - Search query parameters
   * @returns Array of search results with highlighted matches (camelCase fields)
   */
  async searchChatSessions(query: chatTypes.SearchSessionsQuery): Promise<ChatSearchResult[]> {
    logger.info('Searching chat sessions', {
      component: 'ChatService',
      operation: 'searchChatSessions',
      query: query.q,
      scope: query.scope,
    });

    const params = new URLSearchParams();
    params.append('q', query.q);
    if (query.scope) params.append('scope', query.scope);
    if (query.category_id) params.append('category_id', query.category_id);
    if (query.tags) params.append('tags', query.tags);
    if (query.include_archived !== undefined) params.append('include_archived', query.include_archived.toString());
    if (query.limit) params.append('limit', query.limit.toString());

    const response = await this.client.requestList<chatTypes.ChatSearchResult>(
      `/v1/chat/sessions/search?${params.toString()}`
    );

    return toCamelCase<chatTypes.ChatSearchResult[]>(response);
  }

  // ============================================================================
  // Session Sharing
  // ============================================================================

  /**
   * Share a chat session with users or workspace
   *
   * POST /v1/chat/sessions/:session_id/shares
   *
   * @param sessionId - Session ID
   * @param request - Share request with user_ids, workspace_id, and permission
   * @returns Share response with created share IDs (camelCase fields)
   */
  async shareSession(
    sessionId: string,
    request: chatTypes.ShareSessionRequest
  ): Promise<ShareSessionResponse> {
    logger.info('Sharing chat session', {
      component: 'ChatService',
      operation: 'shareSession',
      sessionId,
      userCount: request.user_ids?.length || 0,
      hasWorkspace: !!request.workspace_id,
      permission: request.permission,
    });

    const response = await this.client.request<chatTypes.ShareSessionResponse>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/shares`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    );

    return toCamelCase(response);
  }

  /**
   * Get all shares for a session
   *
   * GET /v1/chat/sessions/:session_id/shares
   *
   * @param sessionId - Session ID
   * @returns Array of session shares with camelCase fields
   */
  async getSessionShares(sessionId: string): Promise<SessionShare[]> {
    const response = await this.client.requestList<chatTypes.SessionShare>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/shares`
    );

    return toCamelCase<chatTypes.SessionShare[]>(response);
  }

  /**
   * Get chat sessions shared with the current user
   *
   * GET /v1/chat/sessions/shared-with-me
   *
   * @param query - Optional query parameters
   * @returns Array of chat sessions shared with the current user (camelCase fields)
   */
  async getSessionsSharedWithMe(
    query?: chatTypes.ListArchivedQuery
  ): Promise<ChatSessionWithStatus[]> {
    const params = new URLSearchParams();
    if (query?.limit) params.append('limit', query.limit.toString());

    const queryString = params.toString();
    const response = await this.client.requestList<chatTypes.ChatSessionWithStatus>(
      `/v1/chat/sessions/shared-with-me${queryString ? `?${queryString}` : ''}`
    );

    return toCamelCase<chatTypes.ChatSessionWithStatus[]>(response);
  }

  /**
   * Revoke a session share
   *
   * DELETE /v1/chat/sessions/:session_id/shares/:share_id
   *
   * @param sessionId - Session ID
   * @param shareId - Share ID to revoke
   * @param shareType - Type of share ('user' or 'workspace'), defaults to 'user'
   */
  async revokeSessionShare(
    sessionId: string,
    shareId: string,
    shareType: 'user' | 'workspace' = 'user'
  ): Promise<void> {
    logger.info('Revoking session share', {
      component: 'ChatService',
      operation: 'revokeSessionShare',
      sessionId,
      shareId,
      shareType,
    });

    const params = new URLSearchParams();
    params.append('type', shareType);

    return this.client.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/shares/${encodeURIComponent(shareId)}?${params.toString()}`,
      {
        method: 'DELETE',
      }
    );
  }

  // ============================================================================
  // Tags Management
  // ============================================================================

  /**
   * List all chat tags for current tenant
   *
   * GET /v1/chat/tags
   *
   * @returns Array of chat tags with camelCase fields
   */
  async listChatTags(): Promise<ChatTag[]> {
    const response = await this.client.requestList<chatTypes.ChatTag>('/v1/chat/tags');
    return toCamelCase<chatTypes.ChatTag[]>(response);
  }

  /**
   * Create a new chat tag
   *
   * POST /v1/chat/tags
   *
   * @param req - Tag creation request
   * @returns Created chat tag with camelCase fields
   */
  async createChatTag(req: chatTypes.CreateTagRequest): Promise<ChatTag> {
    logger.info('Creating chat tag', {
      component: 'ChatService',
      operation: 'createChatTag',
      name: req.name,
    });

    const response = await this.client.request<chatTypes.ChatTag>('/v1/chat/tags', {
      method: 'POST',
      body: JSON.stringify(req),
    });

    return toCamelCase<chatTypes.ChatTag>(response);
  }

  /**
   * Update a chat tag
   *
   * PUT /v1/chat/tags/:tag_id
   *
   * @param tagId - Tag ID to update
   * @param req - Tag update request
   * @returns Updated chat tag with camelCase fields
   */
  async updateChatTag(tagId: string, req: chatTypes.UpdateTagRequest): Promise<ChatTag> {
    logger.info('Updating chat tag', {
      component: 'ChatService',
      operation: 'updateChatTag',
      tagId,
    });

    const response = await this.client.request<chatTypes.ChatTag>(
      `/v1/chat/tags/${encodeURIComponent(tagId)}`,
      {
        method: 'PUT',
        body: JSON.stringify(req),
      }
    );

    return toCamelCase<chatTypes.ChatTag>(response);
  }

  /**
   * Delete a chat tag
   *
   * DELETE /v1/chat/tags/:tag_id
   *
   * @param tagId - Tag ID to delete
   */
  async deleteChatTag(tagId: string): Promise<void> {
    logger.info('Deleting chat tag', {
      component: 'ChatService',
      operation: 'deleteChatTag',
      tagId,
    });

    return this.client.request<void>(`/v1/chat/tags/${encodeURIComponent(tagId)}`, {
      method: 'DELETE',
    });
  }

  /**
   * Assign tags to a chat session
   *
   * POST /v1/chat/sessions/:session_id/tags
   *
   * @param sessionId - Session ID
   * @param tagIds - Array of tag IDs to assign
   * @returns Array of assigned tags with camelCase fields
   */
  async assignTagsToSession(sessionId: string, tagIds: string[]): Promise<ChatTag[]> {
    logger.info('Assigning tags to session', {
      component: 'ChatService',
      operation: 'assignTagsToSession',
      sessionId,
      tagCount: tagIds.length,
    });

    const payload: chatTypes.AssignTagsRequest = {
      tag_ids: tagIds,
    };

    const response = await this.client.requestList<chatTypes.ChatTag>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/tags`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
      }
    );

    return toCamelCase<chatTypes.ChatTag[]>(response);
  }

  /**
   * Get tags for a chat session
   *
   * GET /v1/chat/sessions/:session_id/tags
   *
   * @param sessionId - Session ID
   * @returns Array of tags assigned to the session with camelCase fields
   */
  async getSessionTags(sessionId: string): Promise<ChatTag[]> {
    const response = await this.client.requestList<chatTypes.ChatTag>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/tags`
    );

    return toCamelCase<chatTypes.ChatTag[]>(response);
  }

  /**
   * Remove a tag from a chat session
   *
   * DELETE /v1/chat/sessions/:session_id/tags/:tag_id
   *
   * @param sessionId - Session ID
   * @param tagId - Tag ID to remove
   */
  async removeTagFromSession(sessionId: string, tagId: string): Promise<void> {
    logger.info('Removing tag from session', {
      component: 'ChatService',
      operation: 'removeTagFromSession',
      sessionId,
      tagId,
    });

    return this.client.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/tags/${encodeURIComponent(tagId)}`,
      {
        method: 'DELETE',
      }
    );
  }

  // ============================================================================
  // Categories Management
  // ============================================================================

  /**
   * List all chat categories for current tenant
   *
   * GET /v1/chat/categories
   *
   * @returns Array of chat categories (tree-sorted by path) with camelCase fields
   */
  async listChatCategories(): Promise<ChatCategory[]> {
    const response = await this.client.requestList<chatTypes.ChatCategory>('/v1/chat/categories');
    return toCamelCase<chatTypes.ChatCategory[]>(response);
  }

  /**
   * Create a new chat category
   *
   * POST /v1/chat/categories
   *
   * @param req - Category creation request
   * @returns Created category with camelCase fields
   */
  async createChatCategory(req: chatTypes.CreateCategoryRequest): Promise<ChatCategory> {
    logger.info('Creating chat category', {
      component: 'ChatService',
      operation: 'createChatCategory',
      name: req.name,
      parent_id: req.parent_id,
    });

    const response = await this.client.request<chatTypes.ChatCategory>('/v1/chat/categories', {
      method: 'POST',
      body: JSON.stringify(req),
    });

    return toCamelCase<chatTypes.ChatCategory>(response);
  }

  /**
   * Update a chat category
   *
   * PUT /v1/chat/categories/:category_id
   *
   * @param categoryId - Category ID
   * @param req - Category update request
   * @returns Updated category with camelCase fields
   */
  async updateChatCategory(
    categoryId: string,
    req: chatTypes.UpdateCategoryRequest
  ): Promise<ChatCategory> {
    logger.info('Updating chat category', {
      component: 'ChatService',
      operation: 'updateChatCategory',
      categoryId,
    });

    const response = await this.client.request<chatTypes.ChatCategory>(
      `/v1/chat/categories/${encodeURIComponent(categoryId)}`,
      {
        method: 'PUT',
        body: JSON.stringify(req),
      }
    );

    return toCamelCase<chatTypes.ChatCategory>(response);
  }

  /**
   * Delete a chat category
   *
   * DELETE /v1/chat/categories/:category_id
   *
   * @param categoryId - Category ID
   */
  async deleteChatCategory(categoryId: string): Promise<void> {
    logger.info('Deleting chat category', {
      component: 'ChatService',
      operation: 'deleteChatCategory',
      categoryId,
    });

    return this.client.request<void>(
      `/v1/chat/categories/${encodeURIComponent(categoryId)}`,
      {
        method: 'DELETE',
      }
    );
  }

  /**
   * Set the category for a chat session
   *
   * PUT /v1/chat/sessions/:session_id/category
   *
   * @param sessionId - Session ID
   * @param categoryId - Category ID (or null to clear)
   */
  async setSessionCategory(sessionId: string, categoryId: string | null): Promise<void> {
    logger.info('Setting session category', {
      component: 'ChatService',
      operation: 'setSessionCategory',
      sessionId,
      categoryId,
    });

    const payload: chatTypes.SetCategoryRequest = {
      category_id: categoryId,
    };

    return this.client.request<void>(
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/category`,
      {
        method: 'PUT',
        body: JSON.stringify(payload),
      }
    );
  }

  // ============================================================================
  // Owner System Chat
  // ============================================================================

  /**
   * Send a message to the Owner System Chat endpoint
   *
   * POST /v1/chat/owner-system
   *
   * @param messages - Array of chat messages with role and content
   * @param context - Optional context (route, metrics_snapshot, user_role)
   * @returns Response with message, optional CLI suggestion, and relevant links (camelCase fields)
   */
  async sendOwnerChatMessage(
    messages: ownerTypes.OwnerChatMessage[],
    context?: ownerTypes.OwnerChatContext
  ): Promise<any> {
    logger.info('Sending owner chat message', {
      component: 'ChatService',
      operation: 'sendOwnerChatMessage',
      messageCount: messages.length,
      hasContext: !!context,
    });

    const request: ownerTypes.OwnerChatRequest = { messages, context };
    const response = await this.client.request('/v1/chat/owner-system', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });

    return toCamelCase(response);
  }

  // ============================================================================
  // Real-time Message Subscriptions (SSE)
  // ============================================================================

  /**
   * Subscribe to real-time message updates for a workspace via Server-Sent Events (SSE)
   *
   * GET /v1/workspaces/:workspaceId/messages/stream (SSE endpoint)
   *
   * Note: This uses authTypes.Message (workspace messages), not chatTypes.ChatMessage (chat session messages)
   *
   * @param workspaceId - Workspace ID to subscribe to
   * @param onMessage - Callback invoked with message data (camelCase), or null on connection failure
   * @returns Cleanup function to unsubscribe
   */
  subscribeToMessages(
    workspaceId: string,
    onMessage: (data: { messages: any[] } | null) => void
  ): () => void {
    const url = this.client.buildUrl(`/v1/workspaces/${encodeURIComponent(workspaceId)}/messages/stream`);

    logger.info('Subscribing to messages via SSE', {
      component: 'ChatService',
      operation: 'subscribeToMessages',
      workspaceId,
    });

    let eventSource: EventSource | null = null;

    try {
      eventSource = new EventSource(url, { withCredentials: true });

      eventSource.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data) as { messages: authTypes.Message[] };
          // Transform snake_case response to camelCase for frontend
          const transformedData = toCamelCase<{ messages: any[] }>(data);
          onMessage(transformedData);
        } catch (parseError) {
          logger.error('Failed to parse SSE message', {
            component: 'ChatService',
            operation: 'subscribeToMessages',
            error: parseError instanceof Error ? parseError.message : String(parseError),
          });
        }
      };

      eventSource.onerror = (error) => {
        logger.warn('SSE connection error, notifying subscriber', {
          component: 'ChatService',
          operation: 'subscribeToMessages',
          workspaceId,
          error: error instanceof Error ? error.message : 'Unknown SSE error',
        });
        // Notify subscriber of failure so they can fallback to polling
        onMessage(null);
      };
    } catch (error) {
      logger.error('Failed to create EventSource', {
        component: 'ChatService',
        operation: 'subscribeToMessages',
        error: error instanceof Error ? error.message : String(error),
      });
      // Notify subscriber of failure immediately
      onMessage(null);
    }

    // Return cleanup function
    return () => {
      if (eventSource) {
        logger.info('Unsubscribing from messages SSE', {
          component: 'ChatService',
          operation: 'subscribeToMessages',
          workspaceId,
        });
        eventSource.close();
        eventSource = null;
      }
    };
  }
}
