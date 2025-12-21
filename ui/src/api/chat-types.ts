// Chat API types - migrated to use generated types where available
// Backend: crates/adapteros-db/src/chat_sessions.rs
// Migration: migrations/0085_chat_sessions.sql
// 【2025-11-25†prd-ux-01†chat_api_types】
// 【2025-12-19†migration-to-generated-types】

import type { components } from './generated';

// =============================================================================
// Core Chat Types (from generated API schema)
// =============================================================================

/**
 * Chat session record from database
 * Maps to: ChatSession struct in adapteros-db
 * @see components["schemas"]["ChatSession"]
 */
export type ChatSession = components['schemas']['ChatSession'];

/**
 * Chat message record from database
 * Maps to: ChatMessage struct in adapteros-db
 * @see components["schemas"]["ChatMessage"]
 */
export type ChatMessage = components['schemas']['ChatMessage'];

/**
 * API response wrapper for ChatMessage with guaranteed timestamp
 * @see components["schemas"]["ChatMessageResponse"]
 */
export type ChatMessageResponse = components['schemas']['ChatMessageResponse'];

/**
 * Request to create a new chat session
 * POST /v1/chat/sessions
 * @see components["schemas"]["CreateChatSessionRequest"]
 */
export type CreateChatSessionRequest = components['schemas']['CreateChatSessionRequest'] & {
  /** Optional document name for document chat sessions (UI extension) */
  document_name?: string;
  /** Optional metadata object (will be JSON.stringify'd to metadata_json) */
  metadata?: Record<string, unknown>;
};

/**
 * Response for chat session creation
 * @see components["schemas"]["CreateChatSessionResponse"]
 */
export type CreateChatSessionResponse = components['schemas']['CreateChatSessionResponse'];

/**
 * Request to add a message to a session
 * POST /v1/chat/sessions/:session_id/messages
 * @see components["schemas"]["AddChatMessageRequest"]
 */
export type AddChatMessageRequest = components['schemas']['AddChatMessageRequest'] & {
  /** Optional metadata object (will be JSON.stringify'd to metadata_json) */
  metadata?: Record<string, unknown>;
};

/**
 * Session summary with counts
 * GET /v1/chat/sessions/:session_id/summary
 * @see components["schemas"]["SessionSummary"]
 */
export type SessionSummary = components['schemas']['SessionSummary'] & {
  /** Trace count is not in generated schema, extended here */
  trace_count?: number;
};

// =============================================================================
// Backward Compatibility Aliases (Deprecated)
// =============================================================================

/**
 * @deprecated Use ChatSessionResponse from generated types instead
 * Kept for backward compatibility during migration
 */
export type ChatSessionResponse = ChatSession;

// =============================================================================
// UI-Only Types (Not in Backend Schema)
// =============================================================================

/**
 * Chat session trace record (for router decisions, adapters, etc.)
 * Maps to: ChatSessionTrace struct in adapteros-db
 * NOTE: Not yet in generated schema
 */
export interface ChatSessionTrace {
  id: number;
  session_id: string;
  trace_type: 'router_decision' | 'adapter' | 'training_job' | 'audit_event';
  trace_id: string;
  created_at: string;
}

/**
 * Evidence item associated with a chat message
 * NOTE: Not yet in generated schema
 */
export interface ChatEvidenceItem {
  document_id: string;
  document_name: string;
  chunk_id: string;
  page_number: number | null;
  text_preview: string;
  relevance_score: number;
  rank: number;
}

/**
 * Query parameters for listing sessions
 * NOTE: Not yet in generated schema
 */
export interface ListSessionsQuery {
  user_id?: string;
  limit?: number;
  source_type?: string;
  document_id?: string;
}

/**
 * Request to update session collection binding
 * PUT /v1/chat/sessions/:session_id/collection
 * NOTE: Not yet in generated schema
 */
export interface UpdateSessionCollectionRequest {
  collection_id: string | null;
}

/**
 * Request to update a chat session (title, bindings, metadata)
 * PUT /v1/chat/sessions/:session_id
 * NOTE: Not yet in generated schema
 */
export interface UpdateChatSessionRequest {
  name?: string;
  title?: string;
  stack_id?: string | null;
  collection_id?: string | null;
  document_id?: string | null;
  source_type?: string;
  metadata_json?: string | null;
  tags_json?: string | null;
}

// =============================================================================
// Tags API Types (Migration 0112)
// NOTE: Not yet in generated schema - will be migrated when backend adds OpenAPI docs
// =============================================================================

/**
 * Chat session tag (tenant-scoped)
 * Maps to: ChatTag struct in adapteros-db
 * TODO: Migrate to generated type when available
 */
export interface ChatTag {
  id: string;
  tenant_id: string;
  name: string;
  color?: string;
  description?: string;
  created_at: string;
  created_by?: string;
}

/**
 * Request to create a new tag
 * POST /v1/chat/tags
 * TODO: Migrate to generated type when available
 */
export interface CreateTagRequest {
  name: string;
  color?: string;
  description?: string;
}

/**
 * Request to update a tag
 * PUT /v1/chat/tags/:tag_id
 * TODO: Migrate to generated type when available
 */
export interface UpdateTagRequest {
  name?: string;
  color?: string;
  description?: string;
}

/**
 * Request to assign tags to a session
 * POST /v1/chat/sessions/:session_id/tags
 * TODO: Migrate to generated type when available
 */
export interface AssignTagsRequest {
  tag_ids: string[];
}

// =============================================================================
// Categories API Types (Migration 0112)
// NOTE: Not yet in generated schema - will be migrated when backend adds OpenAPI docs
// =============================================================================

/**
 * Chat session category (hierarchical with materialized path)
 * Maps to: ChatCategory struct in adapteros-db
 * TODO: Migrate to generated type when available
 */
export interface ChatCategory {
  id: string;
  tenant_id: string;
  parent_id?: string;
  name: string;
  path: string;
  depth: number;
  sort_order: number;
  icon?: string;
  color?: string;
  created_at: string;
}

/**
 * Request to create a new category
 * POST /v1/chat/categories
 * TODO: Migrate to generated type when available
 */
export interface CreateCategoryRequest {
  name: string;
  parent_id?: string;
  icon?: string;
  color?: string;
}

/**
 * Request to update a category
 * PUT /v1/chat/categories/:category_id
 * TODO: Migrate to generated type when available
 */
export interface UpdateCategoryRequest {
  name?: string;
  icon?: string;
  color?: string;
}

/**
 * Request to set session category
 * PUT /v1/chat/sessions/:session_id/category
 * TODO: Migrate to generated type when available
 */
export interface SetCategoryRequest {
  category_id: string | null;
}

// =============================================================================
// Soft Delete / Archive Types (Migration 0113)
// NOTE: Not yet in generated schema
// =============================================================================

/**
 * Session status enum (UI-only)
 */
export type SessionStatus = 'active' | 'archived' | 'deleted';

/**
 * Extended chat session with status fields
 * Maps to: ChatSessionWithStatus struct in adapteros-db
 * TODO: Migrate to generated type when available
 */
export interface ChatSessionWithStatus extends ChatSession {
  category_id?: string;
  status: SessionStatus;
  deleted_at?: string;
  deleted_by?: string;
  archived_at?: string;
  archived_by?: string;
  archive_reason?: string;
  description?: string;
  is_shared: boolean;
}

/**
 * Request to archive a session
 * POST /v1/chat/sessions/:session_id/archive
 * TODO: Migrate to generated type when available
 */
export interface ArchiveSessionRequest {
  reason?: string;
}

/**
 * Query parameters for listing archived/deleted sessions
 * TODO: Migrate to generated type when available
 */
export interface ListArchivedQuery {
  limit?: number;
}

// =============================================================================
// Search Types (Migration 0114)
// NOTE: Not yet in generated schema
// =============================================================================

/**
 * Search result for chat sessions/messages
 * Maps to: ChatSearchResult struct in adapteros-db
 * TODO: Migrate to generated type when available
 */
export interface ChatSearchResult {
  session_id: string;
  session_name: string;
  match_type: 'session' | 'message';
  snippet: string;
  message_id?: string;
  message_role?: string;
  relevance_score: number;
  last_activity_at: string;
}

/**
 * Query parameters for session search
 * GET /v1/chat/sessions/search
 * TODO: Migrate to generated type when available
 */
export interface SearchSessionsQuery {
  q: string;
  scope?: 'sessions' | 'messages' | 'all';
  category_id?: string;
  tags?: string; // Comma-separated tag IDs
  include_archived?: boolean;
  limit?: number;
}

// =============================================================================
// Sharing Types (Migration 0115)
// NOTE: Not yet in generated schema
// =============================================================================

/**
 * Share permission level (UI-only enum)
 */
export type SharePermission = 'view' | 'comment' | 'collaborate';

/**
 * Session share record
 * Maps to: SessionShare struct in adapteros-db
 * TODO: Migrate to generated type when available
 */
export interface SessionShare {
  id: string;
  session_id: string;
  workspace_id?: string;
  shared_with_user_id?: string;
  shared_with_tenant_id?: string;
  permission: SharePermission;
  shared_by: string;
  shared_at: string;
  expires_at?: string;
  revoked_at?: string;
}

/**
 * Request to share a session
 * POST /v1/chat/sessions/:session_id/shares
 * TODO: Migrate to generated type when available
 */
export interface ShareSessionRequest {
  user_ids?: string[];
  workspace_id?: string;
  permission: SharePermission;
  expires_at?: string;
}

/**
 * Response from share creation
 * TODO: Migrate to generated type when available
 */
export interface ShareSessionResponse {
  shares: Array<{
    type: 'workspace' | 'user';
    id: string;
    user_id?: string;
  }>;
}

// =============================================================================
// Extended List Query Types
// NOTE: Not yet in generated schema
// =============================================================================

/**
 * Enhanced query parameters for listing sessions
 * GET /v1/chat/sessions
 * TODO: Migrate to generated type when available
 */
export interface EnhancedListSessionsQuery extends ListSessionsQuery {
  category_id?: string;
  tags?: string; // Comma-separated tag IDs
  status?: 'active' | 'archived' | 'all';
  sort_by?: 'last_activity' | 'created' | 'name';
  order?: 'asc' | 'desc';
}
