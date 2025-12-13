// Adapter-related type definitions
// Extracted from types.ts to improve maintainability
//
// 【2025-01-20†refactor†adapter_types】

import React from 'react';
import type { DatasetVersionTrustSnapshot } from './training-types';

// Behavior training types
export interface BehaviorEvent {
  id: string;
  event_type: 'promoted' | 'demoted' | 'evicted' | 'pinned' | 'recovered' | 'ttl_expired';
  adapter_id: string;
  tenant_id: string;
  from_state: string;
  to_state: string;
  activation_pct: number;
  memory_mb: number;
  reason: string;
  created_at: string;
  metadata?: string;
}

export interface BehaviorEventFilters {
  event_type?: string;
  adapter_id?: string;
  tenant_id?: string;
  since?: string;
  until?: string;
  limit?: number;
  offset?: number;
}

export interface BehaviorExportRequest {
  categories?: string[];
  since?: string;
  until?: string;
  synthetic_count?: number;
  min_per_category?: number;
  tenant_id?: string;
  adapter_id?: string;
}

export interface BehaviorStats {
  total_events: number;
  by_category: Record<string, number>;
  by_state_transition: Array<{ from: string; to: string; count: number }>;
}

export interface StageContent {
  id: string;
  title: string;
  description?: string;
  component?: string;
  mockComponent?: React.ComponentType;
  data?: Record<string, unknown>;
}

export interface Adapter {
  id: string;
  adapter_id: string;
  name: string;
  tenant_id?: string;
  hash_b3: string;
  rank: number;
  // Storage tier: 'persistent', 'warm', or 'ephemeral'
  tier: string;
  // Assurance tier for drift/determinism gates
  assurance_tier?: 'low' | 'standard' | 'high';
  // Supported programming languages
  languages?: string[];
  // Languages in JSON string format (for backward compatibility)
  languages_json?: string;
  framework?: string;

  // Semantic naming fields
  adapter_name?: string;           // Full semantic name: tenant/domain/purpose/r001
  tenant_namespace?: string;       // e.g., "shop-floor"
  domain?: string;                 // e.g., "hydraulics"
  purpose?: string;                // e.g., "troubleshooting"
  revision?: string;               // e.g., "r042"
  version?: string;                // e.g., "1.0.0"
  parent_id?: string;              // Parent adapter for lineage tracking
  fork_type?: 'independent' | 'extension';
  fork_reason?: string;

  // Code intelligence fields
  category?: 'code' | 'framework' | 'codebase' | 'ephemeral';
  scope?: AdapterScope;
  lora_tier?: 'micro' | 'standard' | 'max';
  lora_strength?: number;
  lora_scope?: string;
  framework_id?: string;
  framework_version?: string;
  repo_id?: string;
  commit_sha?: string;
  intent?: string;
  base_model_id?: string;
  adapter_trust_state?: 'allowed' | 'warn' | 'blocked' | 'unknown' | 'blocked_regressed';
  dataset_version_ids?: string[];
  dataset_version_trust?: DatasetVersionTrustSnapshot[];

  // Lifecycle state management
  current_state?: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  lifecycle_state?: LifecycleState | AdapterState | string; // Release lifecycle state or legacy runtime aliases
  runtime_state?: string;          // Memory/runtime status
  kv_consistent?: boolean;         // SQL+KV alignment
  kv_message?: string;             // Reason when inconsistent
  pinned?: boolean;
  memory_bytes?: number;
  last_activated?: string;
  activation_count?: number;
  // Drift/determinism metadata recorded by harness
  drift_reference_backend?: string;
  drift_baseline_backend?: string;
  drift_test_backend?: string;
  drift_tier?: 'low' | 'standard' | 'high';
  drift_metric?: number;
  drift_loss_metric?: number;
  drift_slice_size?: number;
  drift_slice_offset?: number;
  manifest_schema_version?: string;
  content_hash_b3?: string;
  signature_valid?: boolean;

  created_at: string;
  updated_at?: string;
  active?: boolean;
  state?: AdapterState;
  last_inference?: string;
  error_count?: number;

  // UI compatibility fields
  status?: 'active' | 'inactive' | 'loading' | 'error';  // Alias for current_state in UI
  description?: string;  // Adapter description
  // CoreML export + verification (PRD-3/PRD-5 placeholders)
  coreml_export_available?: boolean;
  coreml_export_status?: string;
  coreml_export_verified?: boolean;
  coreml_verification_status?: string;
  coreml_export_last_verified_at?: string;
  coreml_export_last_exported_at?: string;
}

// CoreML package/export state (minimal surface for UI)
export interface CoremlPackageStatus {
  supported?: boolean;
  export_available?: boolean;
  export_status?: string;
  export_last_exported_at?: string;
  verification_status?: 'passed' | 'failed' | 'pending' | 'unknown' | string;
  verified?: boolean;
  verified_at?: string;
  coreml_package_hash?: string;
  coreml_expected_package_hash?: string;
  coreml_hash_mismatch?: boolean;
  notes?: string[];
}

export interface CoremlPackageStatusResponse {
  schema_version?: string;
  status: CoremlPackageStatus;
  message?: string;
}

export interface CoremlPackageActionResponse {
  schema_version?: string;
  status?: CoremlPackageStatus;
  message?: string;
}

export type AdapterCategory = 'code' | 'framework' | 'codebase' | 'ephemeral';
export type AdapterScope = 'global' | 'tenant' | 'repo' | 'commit' | 'project';
export type AdapterState =
  | 'unloaded'
  | 'loading'
  | 'cold'
  | 'warm'
  | 'hot'
  | 'resident'
  | 'error';
export type LifecycleState =
  | 'draft'
  | 'training'
  | 'ready'
  | 'active'
  | 'deprecated'
  | 'retired'
  | 'failed';
export type EvictionPriority = 'never' | 'low' | 'normal' | 'high' | 'critical';

export interface ModelInfo {
  id: string;
  name?: string;
  size_mb?: number;
  quantization?: string;
  loaded?: boolean;
  // OpenAI compatible fields
  object?: string;
  created?: number;
  owned_by?: string;
}

export interface RegisterAdapterRequest {
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  // Storage tier: 'persistent', 'warm', or 'ephemeral'
  tier: string;
  // Supported programming languages
  languages: string[];
  framework?: string;
  // Adapter category: 'code', 'framework', 'codebase', or 'ephemeral'
  category: AdapterCategory;
  // Adapter scope: 'global', 'tenant', 'repo', or 'commit'
  scope?: AdapterScope;
  // Expiration timestamp (ISO 8601 format)
  expires_at?: string;
  metadata_json?: string;
}

export interface UpdateAdapterRequest {
  name?: string;
  // Storage tier: 'persistent', 'warm', or 'ephemeral'
  tier?: string;
  expires_at?: string;
  metadata_json?: string;
}

export interface UpdateAdapterStrengthRequest {
  lora_strength: number;
}

export interface AdapterResponse {
  schema_version: string;
  adapter: Adapter;
}

export interface ListAdaptersResponse {
  schema_version: string;
  adapters: Adapter[];
  total: number;
  page: number;
  page_size: number;
}

export interface LoadAdapterRequest {
  adapter_id: string;
  priority?: EvictionPriority;
}

export interface UnloadAdapterRequest {
  adapter_id: string;
}

export interface AdapterLoadResponse {
  schema_version: string;
  adapter_id: string;
  state: AdapterState;
  vram_mb?: number;
}

export interface AdapterFingerprintResponse {
  schema_version: string;
  adapter_id: string;
  fingerprint: string;
  buffer_size: number;
  last_verified: string;
}

export interface ActiveAdapter {
  adapter_id: string;
  gate: number;  // Q15 quantized gate value
  priority?: EvictionPriority;
  // Optional fields for enriched adapter info (may be included by some endpoints)
  id?: string;  // Alias for adapter_id in some API responses
  name?: string;
  lifecycle_state?: string;
}

export interface AdapterStack {
  id: string;
  name: string;
  adapters?: ActiveAdapter[]; // Frontend representation (with gates)
  adapter_ids?: string[]; // Backend representation (just IDs)
  description?: string;
  created_at: string;
  updated_at: string;
  is_default?: boolean;
  version?: number;
  workflow_type?: 'Parallel' | 'UpstreamDownstream' | 'Sequential';
  lifecycle_state?: string; // active, deprecated, retired, draft
  determinism_mode?: string;
}

export interface LifecycleHistoryEvent {
  id: string;
  entity_id: string;
  version: string;
  lifecycle_state: string;
  previous_lifecycle_state?: string;
  reason?: string;
  initiated_by: string;
  metadata_json?: string;
  created_at: string;
}

export interface CreateAdapterStackRequest {
  name: string;
  adapters: ActiveAdapter[];
  description?: string;
}

export interface UpdateAdapterStackRequest {
  name?: string;
  adapters?: ActiveAdapter[];
  description?: string;
}

export interface AdapterStackResponse {
  schema_version: string;
  stack: AdapterStack;
  warnings?: string[];
}

export interface AdapterStrengthSetting {
  adapter_id: string;
  strength?: number;
}

export interface ListAdapterStacksResponse {
  schema_version: string;
  stacks: AdapterStack[];
  total: number;
}

export interface ActivateStackRequest {
  stack_id: string;
}

export interface DeactivateStackRequest {
  stack_id: string;
}

export interface SetDefaultStackRequest {
  stack_id: string;
}

export interface DefaultStackResponse {
  schema_version: string;
  tenant_id: string;
  stack_id: string;
}

export interface ValidateStackNameRequest {
  name: string;
}

export interface ValidateStackNameResponse {
  schema_version: string;
  valid: boolean;
  message?: string;
  errors?: string[];
}

export interface ValidateAdapterNameResponse {
  valid: boolean;
  error?: string;
  suggestions?: string[];
}

// Adapter detail types
export interface AdapterDetailResponse {
  schema_version: string;
  adapter: Adapter;
  manifest: AdapterManifest;
  metrics: AdapterMetrics;
  lineage?: AdapterLineage;
  current_state?: AdapterState;
  runtime_state?: AdapterState;
  lifecycle_state?: string;
  tenant_id?: string;
  tenant_namespace?: string;
  revision?: string;
  last_activated?: string;
  framework?: string;
  // Additional flat fields from adapter
  adapter_name?: string;
  name?: string;
  domain?: string;
  purpose?: string;
  base_model_id?: string;
  memory_bytes?: number;
  activation_count?: number;
  hash_b3?: string;
  rank?: number;
  alpha?: number;
  category?: string;
  scope?: string;
  tier?: string;
  lora_tier?: 'micro' | 'standard' | 'max';
  lora_strength?: number;
  lora_scope?: string;
  manifest_schema_version?: string;
  content_hash_b3?: string;
  signature_valid?: boolean;
}

export interface AdapterManifest {
  version: string;
  name: string;
  description?: string;
  base_model: string;
  rank: number;
  alpha: number;
  target_modules: string[];
  created_at: string;
  hash: string;
  quantization?: string;
  dtype?: string;
  lora_tier?: 'micro' | 'standard' | 'max';
  scope?: string;
}

export interface AdapterMetrics {
  adapter_id?: string;
  inference_count: number;
  total_tokens: number;
  avg_latency_ms: number;
  error_count: number;
  last_used?: string;
  performance?: Record<string, number>;
}

export interface AdapterLineage {
  parent_id?: string;
  children: string[];
  training_job_id?: string;
  dataset_id?: string;
}

export interface AdapterLineageNode {
  adapter_id: string;
  adapter_name?: string;
  revision?: string;
  current_state?: string;
  fork_type?: string;
}

export interface AdapterLineageResponse {
  schema_version: string;
  adapter_id: string;
  lineage: AdapterLineage;
  history: AdapterHistoryEntry[];
  descendants?: AdapterLineageNode[];
  ancestors?: AdapterLineageNode[];
  self_node?: AdapterLineageNode;
  total_nodes?: number;
}

export interface AdapterHistoryEntry {
  timestamp: string;
  action: string;
  actor: string;
  details?: Record<string, unknown>;
}

// Policy types for adapters
export interface CategoryPolicy {
  category?: string;
  allowed_adapters?: string[];
  default_adapter?: string;
  rules?: PolicyRule[];
  promotion_threshold_ms: number;
  demotion_threshold_ms: number;
  memory_limit: number;
  eviction_priority: EvictionPriority;
  auto_promote: boolean;
  auto_demote: boolean;
  max_in_memory: number;
  routing_priority: number;
}

export interface PolicyRule {
  condition: string;
  action: 'allow' | 'deny' | 'require_approval';
  priority: number;
}

export interface Policy {
  id: string;
  name: string;
  type: string;
  content: string;
  status: 'draft' | 'active' | 'archived';
  created_at: string;
  updated_at: string;
  signature?: string;
  cpid?: string;
  schema_hash?: string;
  policy_json?: string;
  enabled?: boolean;
  priority?: number;
  policies?: Policy[];
}

export interface ValidatePolicyRequest {
  policy_json: string;
  policy_type?: string;
}

export interface ApplyPolicyRequest {
  cpid: string;
  content: string;
}

// Adapter stats and state types
export interface AdapterStats {
  adapter_id: string;
  total_inferences: number;
  total_tokens: number;
  avg_latency_ms: number;
  error_rate: number;
  last_24h_inferences: number;
}

export interface AdapterUsageResponse {
  adapter_id: string;
  call_count: number;
  average_gate_value: number;
  last_used: string | null;
}

export interface AdapterActivation {
  adapter_id: string;
  activation_percent: number;
  trend: 'increasing' | 'stable' | 'decreasing';
  history: Array<{ timestamp: string; value: number }>;
}

export interface AdapterStateResponse {
  schema_version: string;
  adapter_id: string;
  current_state: AdapterState;
  previous_state?: AdapterState;
  transition_time?: string;
  reason?: string;
  old_state?: AdapterState;
  new_state?: AdapterState;
}

export type AdapterHealthFlag = 'healthy' | 'degraded' | 'unsafe' | 'corrupt';

export type AdapterHealthDomain =
  | 'drift'
  | 'trust'
  | 'storage'
  | 'other';

export interface AdapterHealthSubcode {
  domain: AdapterHealthDomain;
  code: string;
  message?: string;
  data?: Record<string, unknown>;
}

export interface AdapterDriftSummary {
  current: number;
  hard_threshold?: number;
  tier?: string;
}

export interface AdapterDatasetHealth {
  dataset_version_id: string;
  trust_state: string;
  overall_trust_status?: string;
}

export interface AdapterStorageHealth {
  reconciler_status: string;
  last_checked_at?: string;
  issues?: AdapterHealthSubcode[];
}

export interface AdapterBackendHealth {
  backend?: string;
  coreml_device_type?: string;
  coreml_used?: boolean;
}

export interface AdapterHealthResponse {
  schema_version: string;
  adapter_id: string;
  health: AdapterHealthFlag;
  primary_subcode?: AdapterHealthSubcode;
  subcodes: AdapterHealthSubcode[];
  drift_summary?: AdapterDriftSummary;
  datasets: AdapterDatasetHealth[];
  storage?: AdapterStorageHealth;
  backend?: AdapterBackendHealth;
  recent_activations: AdapterActivationEvent[];
  total_activations: number;
  selected_count: number;
  avg_gate_value: number;
  memory_usage_mb: number;
  policy_violations: string[];
}

export interface UpdateAdapterPolicyRequest {
  adapter_id?: string;
  policy_ids?: string[];
  category?: AdapterCategory;
}

export interface UpdateAdapterPolicyResponse {
  schema_version: string;
  adapter_id: string;
  applied_policies: string[];
  updated_at: string;
}

// Lifecycle types
export interface LifecycleTransitionResponse {
  schema_version: string;
  adapter_id: string;
  from_state: AdapterState;
  to_state: AdapterState;
  success: boolean;
  timestamp: string;
  reason?: string;
}

// Domain adapter types
export interface DomainAdapter {
  id: string;
  domain: string;
  name: string;
  description?: string;
  adapter_ids: string[];
  config: Record<string, unknown>;
  created_at: string;
  updated_at: string;

  // Extended properties for DomainAdapterManager
  model?: string;
  hash?: string;
  input_format?: string;
  output_format?: string;
  version?: string;
  domain_type?: string;
  status?: 'active' | 'inactive' | 'loading' | 'error';
  epsilon_stats?: {
    mean_error: number;
    max_error?: number;
    min_error?: number;
    std_dev?: number;
  };
  execution_count?: number;
  last_execution?: string;
}

export interface CreateDomainAdapterRequest {
  domain: string;
  name: string;
  description?: string;
  adapter_ids: string[];
  config?: Record<string, unknown>;
}

export interface TestDomainAdapterResponse {
  schema_version: string;
  domain_adapter_id: string;
  test_results: Array<{
    test_name: string;
    passed: boolean;
    latency_ms: number;
    output?: string;
    error?: string;
  }>;
  overall_passed: boolean;
  expected_output?: string;
  passed?: boolean;
  actual_output?: string;
  test_id?: string;
  execution_time_ms?: number;
}

export interface DomainAdapterManifest {
  domain: string;
  version: string;
  adapters: Array<{
    id: string;
    weight: number;
    role: string;
  }>;
  routing_strategy: string;
}

export interface DomainAdapterExecutionResponse {
  schema_version: string;
  domain_adapter_id: string;
  execution_id: string;
  result: unknown;
  adapters_invoked: string[];
  total_latency_ms: number;
  tokens_used: number;
}

// Monitoring types
export interface MonitoringRule {
  id: string;
  name: string;
  condition: string;
  threshold: number;
  action: 'alert' | 'scale' | 'restart';
  enabled: boolean;
  created_at: string;
  threshold_operator?: string;
  threshold_value?: number;
  metric_name?: string;
  is_active?: boolean;
  evaluation_window_seconds?: number;
  severity?: 'low' | 'medium' | 'high' | 'critical';
}

export interface CreateMonitoringRuleRequest {
  // Required fields (backend enforced)
  name: string;
  tenant_id: string;
  rule_type: string;
  metric_name: string;
  threshold_value: number;
  threshold_operator: string;
  severity: 'low' | 'medium' | 'high' | 'critical' | 'info';
  evaluation_window_seconds: number;
  cooldown_seconds: number;
  is_active: boolean;
  // Optional fields
  description?: string;
  notification_channels?: Record<string, unknown>;
}

export interface AdapterOSStatus {
  adapter_id: string;
  os_status: 'active' | 'inactive' | 'error';
  last_health_check?: string;
  metrics?: AdapterMetrics;
  services?: Array<{
    id: string;
    name: string;
    status: string;
    state?: string;
    restart_count?: number;
    last_error?: string;
  }>;
}

export interface AdapterStateRecord {
  adapter_id: string;
  state: AdapterState;
  timestamp: string;
  reason?: string;
  category?: string;
  memory_bytes?: number;
  pinned?: boolean;
}

export interface AdapterScore {
  adapter_id: string;
  score: number;
  rank?: number;
  gate_value?: number;
}

export interface AdapterActivationEvent {
  adapter_id: string;
  event_type: 'activated' | 'deactivated' | 'promoted' | 'demoted';
  timestamp: string;
  reason?: string;
}

export interface BatchInferItemResponse {
  schema_version: string;
  id: string;
  text: string;
  response?: string;
  tokens: number;
  latency_ms: number;
  error?: string;
}

export interface AdapterTransitionEvent {
  adapter_id: string;
  from_state: string;
  to_state: string;
  timestamp: string;
  reason?: string;
}

export interface AdapterEvictionEvent {
  adapter_id: string;
  evicted_at: string;
  reason: string;
  memory_freed_bytes?: number;
}

// Memory usage tracking by adapter category
export type MemoryUsageByCategory = Record<AdapterCategory, number>;

// Policy preflight check types (【2025-11-25†ui†stack-preflight-checks】)
export interface PolicyCheck {
  policy_id: string;
  policy_name: string;
  passed: boolean;
  severity: 'error' | 'warning' | 'info';
  message: string;
  can_override?: boolean;
  details?: string;
}

export interface PolicyPreflightResponse {
  checks: PolicyCheck[];
  can_proceed: boolean;
  stack_id?: string;
  adapter_ids?: string[];
}

// Training provenance export types
export interface TrainingExportAdapter {
  id: string;
  name: string;
  version: string;
  base_model: string;
  rank: number;
  alpha: number;
  created_at: string;
}

export interface TrainingExportJob {
  id: string;
  config_hash: string;
  training_config: Record<string, unknown>;
  started_at: string;
  completed_at?: string;
  status: string;
}

export interface TrainingExportDataset {
  id: string;
  name: string;
  hash: string;
  source_location?: string;
}

export interface TrainingExportDocument {
  id: string;
  name: string;
  hash: string;
  page_count?: number;
  created_at: string;
}

export interface TrainingExportConfigVersions {
  chunking_config?: Record<string, unknown>;
  training_config?: Record<string, unknown>;
}

export interface TrainingProvenanceExportResponse {
  schema_version: string;
  adapter: TrainingExportAdapter;
  training_jobs: TrainingExportJob[];
  datasets: TrainingExportDataset[];
  documents: TrainingExportDocument[];
  config_versions: TrainingExportConfigVersions;
  export_timestamp: string;
  export_hash: string;
}

export interface ClearStackAdaptersResponse {
  message: string;
  stack_id: string;
  previous_adapter_count: number;
  adapters_removed: string[];
}

// ============================================================================
// Adapter Publish + Attach Modes v1
// ============================================================================

/**
 * Attach mode controls how an adapter can be attached to inference stacks.
 * - 'free': Adapter can be attached without specific dataset context
 * - 'requires_dataset': Adapter requires a specific dataset version context
 */
export type AttachMode = 'free' | 'requires_dataset';

/**
 * Request to publish an adapter version after training.
 */
export interface PublishAdapterRequest {
  /** Display name for the published adapter (optional) */
  name?: string;
  /** Short description for the adapter version (max 280 chars) */
  short_description?: string;
  /** Attach mode: controls whether dataset context is required */
  attach_mode: AttachMode;
  /** Required dataset version ID when attach_mode is 'requires_dataset' */
  required_scope_dataset_version_id?: string;
}

/**
 * Response from publishing an adapter version.
 */
export interface PublishAdapterResponse {
  schema_version: string;
  version_id: string;
  repo_id: string;
  attach_mode: AttachMode;
  required_scope_dataset_version_id?: string;
  published_at: string;
  short_description?: string;
}

/**
 * Request to archive an adapter version.
 */
export interface ArchiveAdapterRequest {
  /** Reason for archiving (optional, for audit trail) */
  reason?: string;
}

/**
 * Response from archive/unarchive operations.
 */
export interface ArchiveAdapterResponse {
  version_id: string;
  is_archived: boolean;
  updated_at: string;
}

/**
 * Extended adapter type with attach mode information.
 */
export interface AdapterWithAttachMode extends Adapter {
  /** Attach mode for this adapter version */
  attach_mode?: AttachMode;
  /** Required dataset version ID when attach_mode is 'requires_dataset' */
  required_scope_dataset_version_id?: string;
  /** Whether this adapter version is archived */
  is_archived?: boolean;
  /** Timestamp when adapter was published */
  published_at?: string;
  /** Short description of the adapter */
  short_description?: string;
}

// Re-export commonly used types for convenience
export type { Adapter as default };
