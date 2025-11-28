// Chat API types matching backend schema
// Backend: crates/adapteros-db/src/chat_sessions.rs
// Migration: migrations/0085_chat_sessions.sql
// 【2025-11-25†prd-ux-01†chat_api_types】

/**
 * Chat session record from database
 * Maps to: ChatSession struct in adapteros-db
 */
export interface ChatSession {
  id: string;
  tenant_id: string;
  user_id?: string;
  stack_id?: string;
  collection_id?: string;
  name: string;
  created_at: string; // ISO8601 datetime
  last_activity_at: string; // ISO8601 datetime
  metadata_json?: string; // JSON string for additional metadata
}

/**
 * Chat message record from database
 * Maps to: ChatMessage struct in adapteros-db
 */
export interface ChatMessage {
  id: string;
  session_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: string; // ISO8601 datetime
  metadata_json?: string; // JSON string for router decisions, evidence, etc.
}

/**
 * Chat session trace record (for router decisions, adapters, etc.)
 * Maps to: ChatSessionTrace struct in adapteros-db
 */
export interface ChatSessionTrace {
  id: number;
  session_id: string;
  trace_type: 'router_decision' | 'adapter' | 'training_job' | 'audit_event';
  trace_id: string;
  created_at: string;
}

/**
 * Request to create a new chat session
 * POST /v1/chat/sessions
 */
export interface CreateChatSessionRequest {
  name: string;
  stack_id?: string;
  collection_id?: string;
  metadata?: Record<string, unknown>; // Will be JSON.stringify'd to metadata_json
}

/**
 * Response for chat session creation
 */
export interface CreateChatSessionResponse {
  session_id: string;
  tenant_id: string;
  name: string;
  created_at: string;
}

/**
 * Request to add a message to a session
 * POST /v1/chat/sessions/:session_id/messages
 */
export interface AddChatMessageRequest {
  role: 'user' | 'assistant' | 'system';
  content: string;
  metadata?: Record<string, unknown>; // Will be JSON.stringify'd to metadata_json
}

/**
 * Session summary with counts
 * GET /v1/chat/sessions/:session_id/summary
 */
export interface SessionSummary {
  session: ChatSession;
  message_count: number;
  trace_count: number;
}

/**
 * Query parameters for listing sessions
 */
export interface ListSessionsQuery {
  user_id?: string;
  limit?: number;
}

/**
 * Request to update session collection binding
 * PUT /v1/chat/sessions/:session_id/collection
 */
export interface UpdateSessionCollectionRequest {
  collection_id: string | null;
}

// =============================================================================
// Tags API Types (Migration 0112)
// =============================================================================

/**
 * Chat session tag (tenant-scoped)
 * Maps to: ChatTag struct in adapteros-db
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
 */
export interface CreateTagRequest {
  name: string;
  color?: string;
  description?: string;
}

/**
 * Request to update a tag
 * PUT /v1/chat/tags/:tag_id
 */
export interface UpdateTagRequest {
  name?: string;
  color?: string;
  description?: string;
}

/**
 * Request to assign tags to a session
 * POST /v1/chat/sessions/:session_id/tags
 */
export interface AssignTagsRequest {
  tag_ids: string[];
}

// =============================================================================
// Categories API Types (Migration 0112)
// =============================================================================

/**
 * Chat session category (hierarchical with materialized path)
 * Maps to: ChatCategory struct in adapteros-db
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
 */
export interface UpdateCategoryRequest {
  name?: string;
  icon?: string;
  color?: string;
}

/**
 * Request to set session category
 * PUT /v1/chat/sessions/:session_id/category
 */
export interface SetCategoryRequest {
  category_id: string | null;
}

// =============================================================================
// Soft Delete / Archive Types (Migration 0113)
// =============================================================================

/**
 * Session status enum
 */
export type SessionStatus = 'active' | 'archived' | 'deleted';

/**
 * Extended chat session with status fields
 * Maps to: ChatSessionWithStatus struct in adapteros-db
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
 */
export interface ArchiveSessionRequest {
  reason?: string;
}

/**
 * Query parameters for listing archived/deleted sessions
 */
export interface ListArchivedQuery {
  limit?: number;
}

// =============================================================================
// Search Types (Migration 0114)
// =============================================================================

/**
 * Search result for chat sessions/messages
 * Maps to: ChatSearchResult struct in adapteros-db
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
// =============================================================================

/**
 * Share permission level
 */
export type SharePermission = 'view' | 'comment' | 'collaborate';

/**
 * Session share record
 * Maps to: SessionShare struct in adapteros-db
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
 */
export interface ShareSessionRequest {
  user_ids?: string[];
  workspace_id?: string;
  permission: SharePermission;
  expires_at?: string;
}

/**
 * Response from share creation
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
// =============================================================================

/**
 * Enhanced query parameters for listing sessions
 * GET /v1/chat/sessions
 */
export interface EnhancedListSessionsQuery extends ListSessionsQuery {
  category_id?: string;
  tags?: string; // Comma-separated tag IDs
  status?: 'active' | 'archived' | 'all';
  sort_by?: 'last_activity' | 'created' | 'name';
  order?: 'asc' | 'desc';
}
