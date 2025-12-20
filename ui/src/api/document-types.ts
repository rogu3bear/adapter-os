/**
 * Document, Collection, and Evidence API types
 *
 * MIGRATION STATUS: This file is being migrated to use generated types from generated.ts
 *
 * REPLACED types (now imported from generated.ts):
 * - Document → components["schemas"]["DocumentResponse"]
 * - Collection → components["schemas"]["CollectionResponse"]
 * - CollectionDetail → components["schemas"]["CollectionDetailResponse"]
 * - CollectionDocumentInfo → components["schemas"]["CollectionDocumentInfo"]
 * - Evidence → components["schemas"]["EvidenceResponse"]
 * - CreateCollectionRequest → components["schemas"]["CreateCollectionRequest"]
 * - AddDocumentRequest → components["schemas"]["AddDocumentRequest"]
 * - CreateEvidenceRequest → components["schemas"]["CreateEvidenceRequest"]
 * - ListEvidenceQuery → components["schemas"]["ListEvidenceQuery"]
 *
 * KEPT types (UI-specific or not in generated types):
 * - DocumentStatus (UI enum)
 * - ProcessDocumentResponse (processing-specific)
 * - DocumentChunk (chunk-specific)
 * - EvidenceType (UI enum)
 * - ConfidenceLevel (UI enum)
 * - EvidenceStatus (UI enum)
 * - SystemSettings and related (UI settings persistence)
 */

import type { components } from './generated';

// ============================================================================
// Generated Type Imports (Direct Replacement)
// ============================================================================

/**
 * Document metadata response from API
 * @deprecated Use generated type directly: components["schemas"]["DocumentResponse"]
 */
export type Document = components["schemas"]["DocumentResponse"] & {
  /** Alias for name (UI compatibility) */
  title?: string;
};

/**
 * Collection summary response from list endpoint
 * @deprecated Use generated type directly: components["schemas"]["CollectionResponse"]
 */
export type Collection = components["schemas"]["CollectionResponse"] & {
  /** Alias for collection_id (UI compatibility) */
  id?: string;
};

/**
 * Collection detail response with documents
 * @deprecated Use generated type directly: components["schemas"]["CollectionDetailResponse"]
 */
export type CollectionDetail = components["schemas"]["CollectionDetailResponse"];

/**
 * Document info within a collection
 * @deprecated Use generated type directly: components["schemas"]["CollectionDocumentInfo"]
 */
export type CollectionDocumentInfo = components["schemas"]["CollectionDocumentInfo"];

/**
 * Evidence entry response
 * @deprecated Use generated type directly: components["schemas"]["EvidenceResponse"]
 */
export type Evidence = components["schemas"]["EvidenceResponse"] & {
  // Extended fields that may be present in some contexts
  tenant_id?: string | null;
  trace_id?: string | null;
  message_id?: string | null;
  status?: EvidenceStatus | null;
  error_code?: string | null;
  bundle_size_bytes?: number | null;
  download_url?: string | null;
  file_name?: string | null;
  content_type?: string | null;
  // Timestamp aliases
  updated_at?: string | null;
};

/**
 * Request to create a new collection
 * @deprecated Use generated type directly: components["schemas"]["CreateCollectionRequest"]
 */
export type CreateCollectionRequest = components["schemas"]["CreateCollectionRequest"];

/**
 * Request to add a document to a collection
 * @deprecated Use generated type directly: components["schemas"]["AddDocumentRequest"]
 */
export type AddDocumentRequest = components["schemas"]["AddDocumentRequest"];

/**
 * Request to create an evidence entry
 * @deprecated Use generated type directly: components["schemas"]["CreateEvidenceRequest"]
 */
export type CreateEvidenceRequest = components["schemas"]["CreateEvidenceRequest"];

/**
 * Query parameters for listing evidence
 * @deprecated Use generated type directly: components["schemas"]["ListEvidenceQuery"]
 */
export type ListEvidenceQuery = components["schemas"]["ListEvidenceQuery"];

// ============================================================================
// UI-Specific Types (Not in Generated Schema)
// ============================================================================

/** Document status indicating indexing state */
export type DocumentStatus = 'processing' | 'indexed' | 'failed';

/** Response from processing a document */
export interface ProcessDocumentResponse {
  schema_version: string;
  document_id: string;
  status: string;
  chunk_count: number;
  indexed_at: string;
}

/** Document chunk with embedding data */
export interface DocumentChunk {
  schema_version: string;
  chunk_id: string;
  document_id: string;
  chunk_index: number;
  text: string;
  embedding: number[] | null;
  metadata: Record<string, unknown> | null;
  created_at: string;
}

/** Valid evidence types */
export type EvidenceType =
  | 'doc'
  | 'ticket'
  | 'commit'
  | 'policy_approval'
  | 'data_agreement'
  | 'review'
  | 'audit'
  | 'other';

/** Confidence level for evidence */
export type ConfidenceLevel = 'high' | 'medium' | 'low';

/** Status for evidence bundle lifecycle */
export type EvidenceStatus = 'queued' | 'building' | 'ready' | 'failed';

// ============================================================================
// Settings Types (UI Persistence)
// ============================================================================

/** General system settings */
export interface GeneralSettings {
  system_name: string;
  environment: string;
  api_base_url: string;
}

/** Server configuration settings */
export interface ServerSettings {
  http_port: number;
  https_port: number | null;
  uds_socket_path: string | null;
  production_mode: boolean;
}

/** Security configuration settings */
export interface SecuritySettings {
  jwt_mode: 'eddsa' | 'hmac';
  token_ttl_seconds: number;
  require_mfa: boolean;
  egress_enabled: boolean;
  require_pf_deny: boolean;
}

/** Performance configuration settings */
export interface PerformanceSettings {
  max_adapters: number;
  max_workers: number;
  memory_threshold_pct: number;
  cache_size_mb: number;
}

/** Complete system settings */
export interface SystemSettings {
  schema_version: string;
  general: GeneralSettings;
  server: ServerSettings;
  security: SecuritySettings;
  performance: PerformanceSettings;
}

/** Request to update settings (partial update supported) */
export interface UpdateSettingsRequest {
  general?: Partial<GeneralSettings>;
  server?: Partial<ServerSettings>;
  security?: Partial<SecuritySettings>;
  performance?: Partial<PerformanceSettings>;
}

/** Response from settings update */
export interface SettingsUpdateResponse {
  schema_version: string;
  success: boolean;
  restart_required: boolean;
  message: string;
}
