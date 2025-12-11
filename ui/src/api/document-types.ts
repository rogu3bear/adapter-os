/**
 * Document, Collection, and Evidence API types
 *
 * Types matching backend schemas from:
 * - crates/adapteros-server-api/src/handlers/documents.rs
 * - crates/adapteros-server-api/src/handlers/collections.rs
 * - crates/adapteros-server-api/src/handlers/evidence.rs
 */

// ============================================================================
// Document Types
// ============================================================================

/** Document status indicating indexing state */
export type DocumentStatus = 'processing' | 'indexed' | 'failed';

/** Document metadata response from API */
export interface Document {
  schema_version: string;
  document_id: string;
  name: string;
  title?: string; // Alias for name (UI compatibility)
  hash_b3: string;
  size_bytes: number;
  mime_type: string;
  storage_path: string;
  status: DocumentStatus;
  chunk_count: number | null;
  tenant_id: string;
  created_at: string;
  updated_at: string | null;
}

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

// ============================================================================
// Collection Types
// ============================================================================

/** Collection summary response from list endpoint */
export interface Collection {
  schema_version: string;
  collection_id: string;
  id?: string; // Alias for collection_id (UI compatibility)
  name: string;
  description: string | null;
  document_count: number;
  tenant_id: string;
  created_at: string;
  updated_at: string | null;
}

/** Document info within a collection */
export interface CollectionDocumentInfo {
  document_id: string;
  name: string;
  size_bytes: number;
  status: string;
  added_at: string;
}

/** Collection detail response with documents */
export interface CollectionDetail extends Omit<Collection, 'document_count'> {
  document_count: number;
  documents: CollectionDocumentInfo[];
}

/** Request to create a new collection */
export interface CreateCollectionRequest {
  name: string;
  description?: string;
}

/** Request to add a document to a collection */
export interface AddDocumentRequest {
  document_id: string;
}

// ============================================================================
// Evidence Types
// ============================================================================

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

/** Evidence entry response */
export interface Evidence {
  id: string;
  dataset_id: string | null;
  adapter_id: string | null;
  evidence_type: EvidenceType;
  reference: string;
  description: string | null;
  confidence: ConfidenceLevel;
  created_by: string | null;
  created_at: string;
  metadata_json: string | null;
}

/** Request to create an evidence entry */
export interface CreateEvidenceRequest {
  dataset_id?: string;
  adapter_id?: string;
  evidence_type: EvidenceType;
  reference: string;
  description?: string;
  confidence?: ConfidenceLevel;
  metadata_json?: string;
}

/** Query parameters for listing evidence */
export interface ListEvidenceQuery {
  dataset_id?: string;
  adapter_id?: string;
  evidence_type?: EvidenceType;
  confidence?: ConfidenceLevel;
  limit?: number;
}

// ============================================================================
// Settings Types (for persistence)
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
