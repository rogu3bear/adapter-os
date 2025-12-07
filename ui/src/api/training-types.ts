// Training and dataset-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-01-20†refactor†training_types】

export interface TrainingJob {
  id: string;
  adapter_name?: string;
  template_id?: string;
  repo_id?: string;
  dataset_id?: string;
  adapter_id?: string;
  config?: TrainingConfig;
  status: TrainingStatus;
  progress_pct?: number;
  loss?: number;
  current_loss?: number;
  current_epoch?: number;
  total_epochs?: number;
  tokens_per_second?: number;
  learning_rate?: number;
  eta_seconds?: number;
  created_at?: string;
  updated_at?: string;
  started_at?: string;
  completed_at?: string;
  error_message?: string;
  output_path?: string;
  checkpoint_path?: string;
  metadata_json?: string;
  progress?: number;
  metrics?: Record<string, number>;
  artifact_path?: string;
  tenant_id?: string;
  stack_id?: string;

  // Provenance tracking
  base_model_id?: string;
  collection_id?: string;
  build_id?: string;
  config_hash_b3?: string;
  weights_hash_b3?: string;

  // Category metadata
  category?: string;
  description?: string;
  language?: string;
  framework_id?: string;
  framework_version?: string;

  // Audit trail (matches Rust TrainingJob)
  initiated_by?: string;
  initiated_by_role?: string;

  // Category-specific metadata (JSON strings from backend)
  symbol_targets_json?: string;
  api_patterns_json?: string;
  repo_scope?: string;
  file_patterns_json?: string;
  exclude_patterns_json?: string;

  // Post-training actions and provenance
  post_actions_json?: string;
  source_documents_json?: string;
}

export type TrainingStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface TrainingConfig {
  adapter_id?: string;
  dataset_id?: string;
  learning_rate: number;
  epochs: number;
  batch_size: number;
  rank: number;
  alpha: number;
  warmup_steps?: number;
  weight_decay?: number;
  gradient_clip?: number;
  max_seq_length?: number;
  gradient_accumulation_steps?: number;
  save_steps?: number;
  eval_steps?: number;
  logging_steps?: number;
  category?: string;
  targets?: string[];
  scope?: string;
  repo_id?: string;
  framework_id?: string;
  framework_version?: string;
  commit_sha?: string;
}

/**
 * Request to start a new training job
 * Matches backend StartTrainingRequest in adapteros-api-types/src/training.rs
 */
export interface StartTrainingRequest {
  // Required fields
  adapter_name: string;           // REQUIRED - semantic name format
  config: TrainingConfigRequest;  // REQUIRED - training configuration

  // Data source references
  template_id?: string;
  repo_id?: string;
  dataset_id?: string;

  // Provenance tracking
  base_model_id?: string;         // Base model used for training
  collection_id?: string;         // Document collection used

  // Category & metadata
  category?: string;              // code, framework, codebase, docs, domain
  description?: string;           // Human-readable description

  // Category-specific configuration
  language?: string;              // Programming language (for code adapters)
  symbol_targets?: string[];      // Symbol targets (for code adapters)
  framework_id?: string;          // Framework ID (for framework adapters)
  framework_version?: string;     // Framework version (for framework adapters)
  api_patterns?: string[];        // API patterns (for framework adapters)
  repo_scope?: string;            // Repository scope (for codebase adapters)
  file_patterns?: string[];       // File patterns to include (for codebase adapters)
  exclude_patterns?: string[];    // File patterns to exclude (for codebase adapters)

  // Post-training actions
  post_actions?: PostActionsRequest;
}

/**
 * Post-training actions configuration
 * Controls what happens after training completes
 */
export interface PostActionsRequest {
  package?: boolean;              // Package adapter after training (default: true)
  register?: boolean;             // Register adapter in registry (default: true)
  create_stack?: boolean;         // Create new stack with adapter (default: true, NOT set as default)
  tier?: string;                  // Tier to assign: persistent, warm, ephemeral (default: warm)
  adapters_root?: string;         // Custom adapters root directory
}

// TrainingConfigRequest matches backend TrainingConfigRequest
export interface TrainingConfigRequest {
  rank: number;
  alpha: number;
  epochs: number;
  learning_rate: number;
  batch_size: number;
  targets?: string[];  // Optional - backend has default targets
  warmup_steps?: number;
  max_seq_length?: number;
  gradient_accumulation_steps?: number;
  // Additional UI fields (sent to backend if supported)
  weight_decay?: number;
  gradient_clip?: number;
  save_steps?: number;
  eval_steps?: number;
  logging_steps?: number;
}

// UI-only fields for form state (not sent to backend directly)
export interface StartTrainingRequestUIExtras {
  directory_root?: string;
  directory_path?: string;
  dataset_path?: string;
}

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
  current_epoch?: number;
  total_epochs?: number;
  validation_loss?: number;
}

export interface Dataset {
  id: string;
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
  // Usage and evidence counts (computed)
  usage_count?: number;
  evidence_count?: number;
  linked_adapters?: string[];
}

export type TrainingDataset = Dataset;

export type DatasetSourceType = 'code_repo' | 'uploaded_files' | 'generated';
export type DatasetValidationStatus = 'draft' | 'validating' | 'valid' | 'invalid' | 'failed';
export type DatasetType = 'training' | 'eval' | 'red_team' | 'logs' | 'other';
export type CollectionMethod = 'manual' | 'sync' | 'api' | 'pipeline' | 'scrape' | 'other';
export type Strictness = 'strict' | 'epsilon-tolerant' | 'relaxed';

export interface CreateDatasetRequest {
  name: string;
  source_type: DatasetSourceType;
  language?: string;
  framework?: string;
  files?: File[];
  repository_url?: string;
  branch?: string;
  commit_hash?: string;
}

export interface DatasetResponse {
  schema_version: string;
  dataset: Dataset;
  upload_url?: string;
}

/**
 * Request to create a dataset from documents or a collection.
 * Exactly one of document_id or collection_id must be provided.
 */
export interface CreateDatasetFromDocumentsRequest {
  document_id?: string;
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

export interface ListDatasetsResponse {
  schema_version: string;
  datasets: Dataset[];
  total: number;
  page: number;
  page_size: number;
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

// Training artifact types
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
}

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

// Golden run comparison types
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

// Training template types
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

// Training session for real-time monitoring
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

// Evidence entries for datasets and adapters
export type EvidenceType = 'doc' | 'ticket' | 'commit' | 'policy_approval' | 'data_agreement' | 'review' | 'audit' | 'other';
export type EvidenceConfidence = 'high' | 'medium' | 'low';

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
// Chat Bootstrap Types
// ============================================================================

/** Response from GET /v1/training/jobs/{id}/chat_bootstrap */
export interface ChatBootstrapResponse {
  /** Whether the training job is ready for chat (completed with stack) */
  ready: boolean;
  /** Stack ID created from training (if ready) */
  stack_id?: string;
  /** Adapter IDs in the stack */
  adapter_ids: string[];
  /** Base model ID used for training */
  base_model?: string;
  /** RAG collection ID if training involved RAG */
  collection_id?: string;
  /** Suggested title for the chat session */
  suggested_chat_title: string;

  // Provenance fields for Bundle E readiness
  /** Training job ID (always present, echoed from path) */
  training_job_id: string;
  /** Training job status ("pending"|"running"|"completed"|"failed"|"cancelled") */
  status: string;
  /** Primary adapter ID from training job (set after training completes) */
  adapter_id?: string;
  /** Training dataset ID */
  dataset_id?: string;
}

/** Request for POST /v1/chats/from_training_job */
export interface CreateChatFromJobRequest {
  /** Training job ID to create chat from */
  training_job_id: string;
  /** Optional override for chat session name */
  name?: string;
  /** Optional metadata JSON for the chat session */
  metadata_json?: string;
}

/** Response from POST /v1/chats/from_training_job */
export interface CreateChatFromJobResponse {
  /** Created chat session ID */
  session_id: string;
  /** Stack ID the session is bound to */
  stack_id: string;
  /** Session name (either provided or generated) */
  name: string;
  /** Creation timestamp */
  created_at: string;

  // Provenance fields for Bundle E readiness
  /** Training job ID (echoed from request for confirmation) */
  training_job_id: string;
  /** Primary adapter ID from the training job */
  adapter_id?: string;
  /** Training dataset ID */
  dataset_id?: string;
  /** RAG collection ID if linked */
  collection_id?: string;
}

