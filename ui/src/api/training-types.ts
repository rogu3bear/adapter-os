// Training and dataset-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-01-20†refactor†training_types】
// 【2025-12-19†migration†use_generated_types】

import type { components } from './generated';
import type { EvidenceType, ConfidenceLevel } from './document-types';

// ============================================================================
// GENERATED TYPE ALIASES - Direct imports from OpenAPI schema
// ============================================================================

// Training Job Types
export type TrainingJobResponse = components['schemas']['TrainingJobResponse'];

// Extended TrainingJob interface with UI-specific fields
// These fields may be computed on the frontend or come from related data
export interface TrainingJob extends TrainingJobResponse {
  // CoreML tracking (may come from backend_attempts or computed)
  coreml_device_type?: string;
  coreml_attempted?: boolean;
  coreml_used?: boolean;

  // Dataset trust info (from dataset version trust snapshot)
  dataset_trust_state?: TrustState;
  dataset_trust_reason?: string;

  // Stack/adapter relationship
  stack_id?: string;

  // Real-time progress metrics (from SSE updates)
  progress?: number;
  loss?: number;
  eta_seconds?: number;

  // Source control info
  branch?: string;

  // User/session info
  initiated_by?: string;

  // Output paths
  output_path?: string;

  // Full config object (embedded training config)
  config?: TrainingConfigFull;

  // Computed metrics summary
  metrics?: TrainingMetrics;

  // Timestamps
  updated_at?: string;

  // Error tracking
  error_category?: TrainingErrorCategory;
  error_detail?: string;

  // Backend attempts tracking (from API response)
  backend_attempts?: BackendAttempt[];

  // Training metrics (from API response)
  loss_curve?: number[];
  drift_metrics?: Record<string, unknown>;

  // Linked dataset (may be populated from join)
  dataset?: Dataset;
}
export type TrainingConfigRequest = components['schemas']['TrainingConfigRequest'];
export type TrainingConfig = TrainingConfigRequest; // Alias for backward compatibility
export type StartTrainingRequest = components['schemas']['StartTrainingRequest'];
export type PostActionsRequest = components['schemas']['PostActionsRequest'];

// Dataset Types
export type DatasetResponse = components['schemas']['DatasetResponse'];
// Note: Backend DatasetResponse has slightly different structure than legacy Dataset
// Consider gradual migration of consumers to use DatasetResponse directly
export interface Dataset {
  id: string;
  dataset_version_id?: string;
  name: string;
  hash_b3: string;
  source_type: DatasetSourceType;
  language?: string;
  framework?: string;
  file_count: number;
  total_size_bytes: number;
  total_tokens: number;
  validation_status: DatasetValidationStatus;
  created_at: string;
  updated_at: string;
  metadata_json?: string;
  sample_count?: number;
  created_by?: string;
  format?: string;
  storage_path?: string;
  validation_errors?: string;
  description?: string;
  // Dataset Lab extensions
  dataset_type?: DatasetType;
  purpose?: string;
  source_location?: string;
  collection_method?: CollectionMethod;
  ownership?: string;
  tenant_id?: string;
  trust_state?: TrustState;
  trust_reason?: string;
  overall_safety_status?: string;
  pii_status?: string;
  toxicity_status?: string;
  leak_status?: string;
  anomaly_status?: string;
  // Usage and evidence counts (computed)
  usage_count?: number;
  evidence_count?: number;
  linked_adapters?: string[];
}

export type DatasetVersionSelection = components['schemas']['DatasetVersionSelection'];
export type DatasetVersionTrustSnapshot = components['schemas']['DatasetVersionTrustSnapshot'];
// Backend validation status (from OpenAPI schema)
export type DatasetValidationStatus = components['schemas']['DatasetValidationStatus'];

// Extended validation status with UI-only states
// 'draft' is used in the UI for in-progress/unsaved datasets
export type ValidationStatus = DatasetValidationStatus | 'draft';

// Chat Bootstrap Types
export type ChatBootstrapResponse = components['schemas']['ChatBootstrapResponse'];
export type CreateChatFromJobRequest = components['schemas']['CreateChatFromJobRequest'];
export type CreateChatFromJobResponse = components['schemas']['CreateChatFromJobResponse'];

// Data Lineage and Branch Classification
export type DataLineageMode = components['schemas']['DataLineageMode'];
export type BranchClassification = components['schemas']['BranchClassification'];

// ============================================================================
// ENUMS FROM OPENAPI SCHEMA
// ============================================================================
// Template literal keeps string unions while sourcing values from OpenAPI enums.
export type TrainingStatus = `${components['schemas']['TrainingStatus']}`;
export type TrustState = `${components['schemas']['TrustState']}`;
export type DatasetSourceType = `${components['schemas']['DatasetSourceType']}`;

// ============================================================================
// TYPE GUARD FUNCTIONS
// ============================================================================
// These functions provide runtime validation for enum values, replacing
// unsafe `as TypeName` assertions with proper type narrowing.

/**
 * Type guard for TrainingStatus enum values.
 * Use this instead of `as TrainingStatus` assertions to validate runtime values.
 *
 * @param value - The unknown value to validate
 * @returns True if the value is a valid TrainingStatus
 *
 * @example
 * ```typescript
 * const status = event.target.value;
 * if (isTrainingStatus(status)) {
 *   setFilter(status); // status is now typed as TrainingStatus
 * }
 * ```
 */
export function isTrainingStatus(value: unknown): value is TrainingStatus {
  return ['pending', 'running', 'completed', 'failed', 'cancelled', 'paused'].includes(value as string);
}

/**
 * Type guard for TrustState enum values.
 * Use this instead of `as TrustState` assertions to validate runtime values.
 *
 * @param value - The unknown value to validate
 * @returns True if the value is a valid TrustState
 *
 * @example
 * ```typescript
 * const trustState = formData.trust_state;
 * if (isTrustState(trustState)) {
 *   updateTrust(trustState); // trustState is now typed as TrustState
 * }
 * ```
 */
export function isTrustState(value: unknown): value is TrustState {
  return ['allowed', 'allowed_with_warning', 'blocked', 'needs_approval', 'unknown'].includes(value as string);
}

/**
 * Type guard for DatasetSourceType enum values.
 * Use this instead of `as DatasetSourceType` assertions to validate runtime values.
 *
 * @param value - The unknown value to validate
 * @returns True if the value is a valid DatasetSourceType
 *
 * @example
 * ```typescript
 * const sourceType = selectValue;
 * if (isDatasetSourceType(sourceType)) {
 *   createDataset({ source_type: sourceType }); // sourceType is now typed as DatasetSourceType
 * }
 * ```
 */
export function isDatasetSourceType(value: unknown): value is DatasetSourceType {
  return ['code_repo', 'uploaded_files', 'generated'].includes(value as string);
}

// ============================================================================
// UI-ONLY TYPES (Not sent to backend)
// ============================================================================

// UI-only fields for form state (not sent to backend directly)
export interface StartTrainingRequestUIExtras {
  directory_root?: string;
  directory_path?: string;
  dataset_path?: string;
}

// ============================================================================
// LEGACY RESPONSE WRAPPERS
// Keep for backward compatibility with existing code
// ============================================================================

export interface TrainingResponse {
  schema_version: string;
  job: TrainingJob;
}

export interface ListTrainingJobsResponse {
  schema_version: string;
  jobs: TrainingJob[];
  total: number;
  page: number;
  page_size: number;
}

export interface ListDatasetsResponse {
  schema_version: string;
  datasets: Dataset[];
  total: number;
  page: number;
  page_size: number;
}

export interface DatasetVersionSummary {
  dataset_version_id: string;
  version_number: number;
  version_label?: string;
  hash_b3?: string;
  storage_path?: string;
  trust_state?: TrustState;
  repo_slug?: string;
  created_at: string;
}

export interface DatasetVersionListResponse {
  schema_version: string;
  dataset_id: string;
  versions: DatasetVersionSummary[];
}

// ============================================================================
// TRAINING METRICS FROM BACKEND
// ============================================================================

/**
 * Individual training metric entry for time-series data.
 * Returned by GET /v1/training/jobs/:jobId/metrics endpoint.
 */
export interface TrainingMetricEntry {
  /** Metric step (training iteration) */
  step: number;
  /** Loss value at this step */
  loss: number;
  /** Learning rate at this step (optional - not stored per-step in current schema) */
  learning_rate?: number;
  /** Training epoch */
  epoch: number;
  /** Tokens processed up to this point */
  tokens_processed?: number;
  /** Timestamp of this metric */
  timestamp: string;
}

/**
 * Training metrics list response for time-series metrics endpoint.
 * Returned by GET /v1/training/jobs/:jobId/metrics endpoint.
 */
export interface TrainingMetricsListResponse {
  schema_version: string;
  /** Training job ID */
  job_id: string;
  /** Time-series metrics */
  metrics: TrainingMetricEntry[];
}

// ============================================================================
// COMPUTED/UI METRICS (Not from backend directly)
// ============================================================================

export interface TrainingMetrics {
  step?: number;
  loss?: number;
  learning_rate?: number;
  epoch?: number;
  tokens_processed?: number;
  tokens_per_second?: number;
  time_elapsed?: number;
  eta_seconds?: number;
  progress_pct?: number;
  memory_usage?: number;
  gpu_utilization?: number;
  backend?: string;
  backend_device?: string;
  using_gpu?: boolean;
  current_epoch?: number;
  total_epochs?: number;
  validation_loss?: number;
  examples_processed?: number;
  training_time_ms?: number;
  throughput_examples_per_sec?: number;
  peak_gpu_memory_mb?: number;
}

// ============================================================================
// CUSTOM BACKEND ATTEMPT TRACKING (UI-specific structure)
// ============================================================================

export type BackendAttemptResult = 'selected' | 'failed' | 'skipped';

export interface BackendAttempt {
  backend: string;
  result?: BackendAttemptResult;
  reason?: string;
  error_category?: TrainingErrorCategory;
  error_code?: string;
  coreml?: {
    attempted?: boolean;
    used?: boolean;
    device_type?: string;
  };
  started_at?: string;
  completed_at?: string;
}

export type TrainingErrorCategory =
  | 'coreml_compile'
  | 'dataset_trust'
  | 'storage'
  | 'backend'
  | 'policy'
  | 'other';

// ============================================================================
// HANDLER-SPECIFIC TYPES (Complex structures not in generated schema)
// ============================================================================

export interface GoldenRunSummary {
  run_id: string;
  job_id: string;
  metrics: {
    final_loss: number;
    best_loss: number;
    total_steps: number;
    tokens_processed: number;
  };
  config: TrainingConfigFull;
  created_at: string;
  has_signature?: boolean;
  name?: string;
  cpid?: string;
  plan_id?: string;
  adapters?: string[];
  layer_count?: number;
  mean_epsilon?: number;
  max_epsilon?: number;
  toolchain_summary?: string;
}

export interface TrainingConfigFull {
  learning_rate: number;
  batch_size: number;
  epochs: number;
  warmup_steps: number;
  weight_decay: number;
  gradient_accumulation_steps: number;
  max_grad_norm: number;
  seed: number;
  category?: string;
  scope?: string;
  repo_id?: string;
  // LoRA parameters
  rank?: number;
  alpha?: number;
  targets?: string[];
  // Advanced settings
  max_seq_length?: number;
  backend_policy?: string;
  coreml_placement?: string;
  coreml_training_fallback?: string;
  enable_coreml_export?: boolean;
  require_gpu?: boolean;
  // Index signature for dynamic access
  [key: string]: unknown;
}

export interface VerificationReport {
  job_id: string;
  status: 'passed' | 'failed' | 'warning';
  passed: boolean;
  checks: VerificationCheck[];
  epsilon_comparison: {
    expected: number;
    actual: number;
    within_tolerance: boolean;
    divergent_layers: LayerDivergence[];
    tolerance?: number;
    pass_rate?: number;
  };
  messages?: string[];
  summary: string;
  generated_at: string;
  toolchain_compatible?: boolean;
  signature_verified?: boolean;
  device_compatible?: boolean;
  bundle_hash_match?: boolean;
  adapters_compatible?: boolean;
}

export interface VerificationCheck {
  name: string;
  status: 'passed' | 'failed' | 'skipped';
  message?: string;
  details?: Record<string, unknown>;
}

export interface LayerDivergence {
  layer_id: string;
  relative_error: number;
  golden: {
    l2_error: number;
    mean_error: number;
    max_error: number;
    element_count?: number;
  };
  current: {
    l2_error: number;
    mean_error: number;
    max_error: number;
    element_count?: number;
  };
  threshold: number;
  passed: boolean;
}

// ============================================================================
// DATASET-RELATED TYPES
// ============================================================================

export type DatasetType = 'training' | 'eval' | 'red_team' | 'logs' | 'other';
export type CollectionMethod = 'manual' | 'sync' | 'api' | 'pipeline' | 'scrape' | 'other';
export type Strictness = 'strict' | 'epsilon-tolerant' | 'relaxed';
export type TrainingDataset = Dataset;

export interface DatasetTrustOverrideRequest {
  override_state: TrustState;
  reason: string;
}

export interface DatasetTrustOverrideResponse {
  dataset_id: string;
  dataset_version_id: string;
  effective_trust_state?: TrustState;
}

export interface CreateDatasetRequest {
  name: string;
  source_type: DatasetSourceType;
  language?: string;
  framework?: string;
  files?: File[];
  description?: string;
  format?: string;
  repository_url?: string;
  branch?: string;
  commit_hash?: string;
}

/**
 * Request to create a dataset from documents or a collection.
 * Exactly one of document_id or collection_id must be provided.
 */
export interface CreateDatasetFromDocumentsRequest {
  document_id?: string;
  document_ids?: string[];
  collection_id?: string;
  name?: string;
  description?: string;
}

/**
 * Response from creating a dataset from documents.
 * Returns flat dataset fields (not wrapped in `dataset`).
 */
export interface CreateDatasetFromDocumentsResponse {
  schema_version: string;
  dataset_id: string;
  dataset_version_id?: string;
  name: string;
  description?: string;
  file_count: number;
  total_size_bytes: number;
  format: string;
  hash: string;
  storage_path: string;
  validation_status: string;
  validation_errors?: string;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface DatasetValidationResult {
  dataset_id: string;
  status: DatasetValidationStatus;
  errors?: string[];
  warnings?: string[];
  stats?: {
    total_files: number;
    valid_files: number;
    total_tokens: number;
    language_breakdown: Record<string, number>;
  };
}

export interface UploadProgress {
  dataset_id: string;
  uploaded_bytes: number;
  total_bytes: number;
  status: 'uploading' | 'processing' | 'completed' | 'failed';
  error_message?: string;
}

// ============================================================================
// TRAINING ARTIFACTS
// ============================================================================

export interface TrainingArtifactsResponse {
  schema_version: string;
  job_id: string;
  artifacts: TrainingArtifact[];
  signature_valid?: boolean;
  ready?: boolean;
  manifest_hash_matches?: boolean;
  adapter_id?: string;
  artifact_path?: string;
  manifest_hash_b3?: string;
  weights_hash_b3?: string;
}

export interface TrainingArtifact {
  id: string;
  type: 'checkpoint' | 'final' | 'log' | 'metrics';
  path: string;
  size_bytes: number;
  created_at: string;
  metadata?: Record<string, unknown>;
}

// ============================================================================
// GOLDEN RUN COMPARISON
// ============================================================================

export interface TrainingComparison {
  baseline: GoldenRunSummary;
  current: GoldenRunSummary;
  metrics_diff: {
    loss_diff: number;
    loss_diff_percent: number;
    steps_diff: number;
  };
}

export interface DatasetValidation {
  dataset_id: string;
  status: 'valid' | 'invalid' | 'warning';
  issues: ValidationIssue[];
  stats: {
    total_samples: number;
    valid_samples: number;
    invalid_samples: number;
  };
}

export interface ValidationIssue {
  type: 'error' | 'warning';
  message: string;
  sample_index?: number;
  field?: string;
}

export interface GoldenMetric {
  key: string;
  value1: number;
  value2: number;
  diff: number;
}

export interface GoldenCompareResult {
  baseline_run_id: string;
  current_run_id: string;
  passed: boolean;
  metrics_comparison: {
    loss_diff: number;
    loss_diff_percent: number;
    threshold: number;
  };
  details?: Record<string, unknown>;
  metrics?: GoldenMetric[];
  summary?: string;
  recommendations?: string[];
}

export interface GoldenCompareRequest {
  golden: string;
  bundle_id: string;
  strictness: Strictness;
  verify_toolchain?: boolean;
  verify_adapters?: boolean;
  verify_signature?: boolean;
  verify_device?: boolean;
  threshold?: number;
  epsilon_tolerance?: number;
}

// ============================================================================
// TRAINING TEMPLATES
// ============================================================================

export interface TrainingTemplate {
  id: string;
  name: string;
  description?: string;
  config?: TrainingConfig;
  target_modules?: string[];
  created_at?: string;
  category?: string;
  default_epochs?: number;
  default_batch_size?: number;
  rank?: number;
  alpha?: number;
  learning_rate?: number;
  epochs?: number;
  batch_size?: number;
  targets?: string[];
}

// ============================================================================
// TRAINING SESSION (Real-time monitoring)
// ============================================================================

export interface TrainingSession {
  session_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  adapter_id?: string;
  adapter_name?: string;
  dataset_id?: string;
  repository_path?: string;
  created_at: string;
  updated_at?: string;
  started_at?: string;
  completed_at?: string;
  progress?: number;
  progress_pct?: number;
  current_epoch?: number;
  total_epochs?: number;
  current_loss?: number;
  tokens_per_second?: number;
  eta_seconds?: number;
  error_message?: string;
  config?: TrainingConfig;
  metrics?: TrainingMetrics;
}

// ============================================================================
// EVIDENCE AND PROVENANCE
// ============================================================================

// Re-export types from document-types to maintain backward compatibility
export type { EvidenceType };
export type EvidenceConfidence = ConfidenceLevel;

export interface EvidenceEntry {
  id: string;
  dataset_id?: string;
  adapter_id?: string;
  evidence_type: EvidenceType;
  reference: string;
  description?: string;
  confidence: EvidenceConfidence;
  created_by?: string;
  created_at: string;
  metadata_json?: string;
}

export interface DatasetAdapterLink {
  id: string;
  dataset_id: string;
  adapter_id: string;
  link_type: 'training' | 'eval' | 'validation' | 'test';
  created_at: string;
}

// ============================================================================
// DRIFT METRICS
// ============================================================================

export interface DriftMetrics {
  drift_score?: number;
  drift_tokens?: number;
  baseline_loss?: number;
  window_seconds?: number;
}

// ============================================================================
// CHUNKED UPLOAD TYPES
// ============================================================================

/**
 * Request to initiate a chunked upload session.
 * Backend: POST /v1/datasets/chunked-upload/initiate
 */
export interface InitiateChunkedUploadRequest {
  /** File name being uploaded */
  file_name: string;
  /** Total file size in bytes */
  total_size: number;
  /** Content type (e.g., application/gzip) */
  content_type?: string;
  /** Chunk size preference (will be clamped to valid range) */
  chunk_size?: number;
  /** Optional workspace ID for tenant isolation */
  workspace_id?: string;
}

/**
 * Response from initiating a chunked upload.
 * Backend: POST /v1/datasets/chunked-upload/initiate
 */
export interface InitiateChunkedUploadResponse {
  /** Unique session identifier */
  session_id: string;
  /** Chunk size that will be used */
  chunk_size: number;
  /** Expected number of chunks */
  expected_chunks: number;
  /** Whether compression is detected */
  compression_format: string;
}

/**
 * Response from uploading a chunk.
 * Backend: POST /v1/datasets/chunked-upload/:sessionId/chunk
 */
export interface UploadChunkResponse {
  /** Session ID */
  session_id: string;
  /** Chunk index that was uploaded */
  chunk_index: number;
  /** BLAKE3 hash of this chunk */
  chunk_hash: string;
  /** Total chunks received so far */
  chunks_received: number;
  /** Total expected chunks */
  expected_chunks: number;
  /** Is upload complete (all chunks received)? */
  is_complete: boolean;
  /** Resume token for resuming from next chunk (if not complete) */
  resume_token?: string;
}

/**
 * Response from retrying a chunk upload.
 * Backend: PUT /v1/datasets/chunked-upload/:sessionId/chunk
 */
export interface RetryChunkResponse {
  /** Session ID */
  session_id: string;
  /** Chunk index that was retried */
  chunk_index: number;
  /** BLAKE3 hash of the new chunk */
  chunk_hash: string;
  /** Previous hash if this was replacing an existing chunk */
  previous_hash?: string;
  /** Total chunks received so far */
  chunks_received: number;
  /** Total expected chunks */
  expected_chunks: number;
  /** Is upload complete (all chunks received)? */
  is_complete: boolean;
  /** Whether this was actually a retry (chunk existed before) */
  was_retry: boolean;
}

/**
 * Request to complete a chunked upload and create the dataset.
 * Backend: POST /v1/datasets/chunked-upload/:sessionId/complete
 */
export interface CompleteChunkedUploadRequest {
  /** Dataset name (optional, defaults to file name) */
  name?: string;
  /** Dataset description */
  description?: string;
  /** Dataset format (e.g., "jsonl", "json", "csv") */
  format?: string;
  /** Optional workspace ID for tenant isolation (should match initiate request) */
  workspace_id?: string;
}

/**
 * Response from completing a chunked upload.
 * Backend: POST /v1/datasets/chunked-upload/:sessionId/complete
 */
export interface CompleteChunkedUploadResponse {
  /** Created dataset ID */
  dataset_id: string;
  /** The dataset version ID created for this upload */
  dataset_version_id?: string;
  /** Dataset name */
  name: string;
  /** Dataset hash (manifest-derived BLAKE3) */
  hash: string;
  /** Total file size in bytes */
  total_size_bytes: number;
  /** Storage path */
  storage_path: string;
  /** Timestamp when dataset was created */
  created_at: string;
  /** Workspace ID if dataset was scoped to a workspace */
  workspace_id?: string;
}

/**
 * Response for getting upload session status.
 * Backend: GET /v1/datasets/chunked-upload/:sessionId/status
 */
export interface UploadSessionStatusResponse {
  /** Session ID */
  session_id: string;
  /** Original file name */
  file_name: string;
  /** Total file size in bytes */
  total_size: number;
  /** Chunk size for this upload */
  chunk_size: number;
  /** Expected number of chunks */
  expected_chunks: number;
  /** Number of chunks received */
  chunks_received: number;
  /** List of chunk indices that have been received */
  received_chunk_indices: number[];
  /** Whether all chunks have been received */
  is_complete: boolean;
  /** Session creation timestamp (RFC3339) */
  created_at: string;
  /** Compression format detected */
  compression_format: string;
}

/**
 * Summary of an upload session for listing.
 * Backend: GET /v1/datasets/chunked-upload/sessions
 */
export interface UploadSessionSummary {
  /** Session ID */
  session_id: string;
  /** Original file name */
  file_name: string;
  /** Total file size in bytes */
  total_size: number;
  /** Number of chunks received */
  chunks_received: number;
  /** Total expected chunks */
  expected_chunks: number;
  /** Upload progress percentage */
  progress_percent: number;
  /** Session creation timestamp (RFC3339) */
  created_at: string;
  /** Age of the session in seconds */
  age_seconds: number;
  /** Whether the session has expired */
  is_expired: boolean;
}

/**
 * Response for listing upload sessions.
 * Backend: GET /v1/datasets/chunked-upload/sessions
 */
export interface ListUploadSessionsResponse {
  /** List of active upload sessions */
  sessions: UploadSessionSummary[];
  /** Total number of active sessions */
  total_count: number;
  /** Maximum allowed concurrent sessions */
  max_sessions: number;
}
