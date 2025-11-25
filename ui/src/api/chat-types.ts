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
