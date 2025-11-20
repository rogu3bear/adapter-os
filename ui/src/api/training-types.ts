// Training and dataset-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-01-20†refactor†training_types】

export interface TrainingJob {
  id: string;
  dataset_id: string;
  adapter_id: string;
  status: TrainingStatus;
  progress_pct?: number;
  loss?: number;
  tokens_per_sec?: number;
  eta_seconds?: number;
  created_at: string;
  updated_at: string;
  completed_at?: string;
  error_message?: string;
  metadata_json?: string;
}

export type TrainingStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface TrainingConfig {
  adapter_id: string;
  dataset_id: string;
  learning_rate: number;
  epochs: number;
  batch_size: number;
  rank: number;
  alpha: number;
  warmup_steps?: number;
  weight_decay?: number;
  gradient_clip?: number;
  save_steps?: number;
  eval_steps?: number;
  logging_steps?: number;
}

export interface StartTrainingRequest {
  config: TrainingConfig;
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
  step: number;
  loss: number;
  learning_rate: number;
  epoch: number;
  tokens_processed: number;
  tokens_per_sec: number;
  time_elapsed: number;
  eta_seconds: number;
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
}

export type DatasetSourceType = 'code_repo' | 'uploaded_files' | 'generated';
export type DatasetValidationStatus = 'pending' | 'validating' | 'valid' | 'invalid' | 'failed';

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

