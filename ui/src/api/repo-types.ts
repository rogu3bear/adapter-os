export type RepoStatus = 'healthy' | 'degraded' | 'archived' | 'unknown';

export interface RepoBranchSummary {
  name: string;
  default?: boolean;
  latest_active_version?: RepoVersionSummary | null;
}

import type { AdapterHealthFlag } from '@/api/adapter-types';
import type { TrustState } from '@/api/training-types';

export interface RepoSummary {
  id: string;
  name: string;
  base_model: string;
  default_branch: string;
  status: RepoStatus;
  branches: RepoBranchSummary[];
  description?: string;
  tags?: string[];
  created_at: string;
  updated_at?: string;
  tenant_id?: string;
  owner_id?: string;
}

export interface RepoDetail extends RepoSummary {
  metadata?: Record<string, unknown>;
  latest_versions?: RepoVersionSummary[];
  metrics?: Record<string, number | null>;
}

export type ReleaseState = 'draft' | 'candidate' | 'active' | 'archived' | 'failed';

export interface RepoVersionSummary {
  id: string;
  version: string;
  branch: string;
  release_state: ReleaseState;
  serveable?: boolean;
  serveable_reason?: string;
  adapter_trust_state?: 'allowed' | 'warn' | 'blocked' | 'unknown' | 'blocked_regressed';
  dataset_version_ids?: string[];
  dataset_version_trust?: { dataset_version_id: string; trust_at_training_time?: string }[];
  scope_path?: string;
  data_spec_hash?: string;
  training_backend?: string;
  coreml_used?: boolean;
  coreml_device_type?: string;
  health_state?: AdapterHealthFlag;
  trust_state?: TrustState;
  metrics?: {
    reward?: number | null;
    latency_p50_ms?: number | null;
    tokens_per_sec?: number | null;
    [key: string]: number | null | undefined;
  };
  tags?: string[];
  created_at: string;
  updated_at?: string;
  aos_hash?: string;
  aos_path?: string;
  commit_sha?: string;
  commit_url?: string;
  data_spec_summary?: string;
}

export interface RepoVersionDetail extends RepoVersionSummary {
  metadata?: Record<string, unknown>;
  base_model?: string;
}

export type RepoTimelineEventType =
  | 'version_promoted'
  | 'version_rolled_back'
  | 'training_started'
  | 'training_failed'
  | 'training_completed'
  | 'tagged'
  | 'archived'
  | 'info';

export interface RepoTimelineEvent {
  id: string;
  timestamp: string;
  type: RepoTimelineEventType;
  title: string;
  description?: string;
  version_id?: string;
  branch?: string;
  job_id?: string;
}

export interface RepoTrainingJobLink {
  id: string;
  version_id?: string;
  status: string;
  created_at: string;
  updated_at?: string;
  metrics?: Record<string, number | null>;
}

export interface CreateRepoRequest {
  name: string;
  base_model: string;
  default_branch?: string;
  description?: string;
  tags?: string[];
}

export interface UpdateRepoRequest {
  description?: string;
  default_branch?: string;
  tags?: string[];
  status?: RepoStatus;
}

export interface PromoteVersionRequest {
  branch?: string;
}

export interface RollbackVersionRequest {
  target_version_id?: string;
  reason?: string;
}

export interface TagVersionRequest {
  tags: string[];
}

export interface StartTrainingFromVersionRequest {
  training_config_id?: string;
  hyperparams?: Record<string, unknown>;
  target_branch?: string;
}

// Adapter repository policy/types (system adapter repos, not code repos)
export type CoreMLMode = 'coreml_strict' | 'coreml_preferred' | 'backend_auto';
export type RepoAssuranceTier = 'high_assurance' | 'normal';

export interface AdapterRepositoryPolicy {
  repo_id: string;
  preferred_backends?: string[];
  coreml_allowed: boolean;
  coreml_required: boolean;
  autopromote_coreml: boolean;
  coreml_mode: CoreMLMode;
  repo_tier: RepoAssuranceTier;
  auto_rollback_on_trust_regress: boolean;
  created_at?: string;
}

export interface UpdateAdapterRepositoryPolicyRequest {
  preferred_backends?: string[];
  coreml_allowed?: boolean;
  coreml_required?: boolean;
  autopromote_coreml?: boolean;
  coreml_mode?: CoreMLMode;
  repo_tier?: RepoAssuranceTier;
  auto_rollback_on_trust_regress?: boolean;
}

export interface AdapterRepositorySummary {
  id: string;
  tenant_id: string;
  name: string;
  base_model_id?: string;
  default_branch: string;
  archived: boolean;
  created_by?: string;
  created_at: string;
  description?: string;
  training_policy?: AdapterRepositoryPolicy | null;
}
