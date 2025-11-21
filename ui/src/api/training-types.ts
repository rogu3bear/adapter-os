// Training and dataset-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-01-20†refactor†training_types】

export interface TrainingJob {
  id: string;
  dataset_id?: string;
  adapter_id?: string;
  adapter_name?: string;
  template_id?: string;
  config?: TrainingConfig;
  status: TrainingStatus;
  progress_pct?: number;
  loss?: number;
  current_loss?: number;
  current_epoch?: number;
  total_epochs?: number;
  tokens_per_second?: number;
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
  learning_rate?: number;
}

export type TrainingStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled' | 'paused';

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

export interface StartTrainingRequest {
  // Required fields (match backend)
  adapter_name: string;           // REQUIRED - semantic name format
  config: TrainingConfigRequest;  // REQUIRED - training configuration

  // Optional fields (match backend)
  template_id?: string;
  repo_id?: string;
  dataset_id?: string;
}

// TrainingConfigRequest matches backend TrainingConfigRequest
export interface TrainingConfigRequest {
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
  targets?: string[];
}

// UI-only fields for StartTrainingRequest (not sent to backend)
export interface StartTrainingRequestUIExtras {
  directory_root?: string;
  directory_path?: string;
  adapters_root?: string;
  dataset_path?: string;
  tenant_id?: string;
  package?: string;
  category?: string;
  language?: string;
}

export interface TrainingResponse {
  job: TrainingJob;
}

export interface ListTrainingJobsResponse {
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
  total_tokens: number;
  validation_status: DatasetValidationStatus;
  created_at: string;
  updated_at: string;
  metadata_json?: string;
  sample_count?: number;
  created_by?: string;
}

export type TrainingDataset = Dataset;

export type DatasetSourceType = 'code_repo' | 'uploaded_files' | 'generated';
export type DatasetValidationStatus = 'pending' | 'validating' | 'valid' | 'invalid' | 'failed';
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
  dataset: Dataset;
  upload_url?: string;
}

export interface ListDatasetsResponse {
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
  status: 'pending' | 'running' | 'completed' | 'failed' | 'paused';
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

