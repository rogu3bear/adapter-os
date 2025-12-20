// General API response and utility types
// Extracted from types.ts for better organization
//
// 【2025-01-20†rectification†api_types】
//
// IMPORTANT: This file contains BACKEND types (snake_case, as received from API).
// For clean FRONTEND types (camelCase), use @/api/domain-types.ts
// For transforming backend → frontend, use @/api/transformers.ts
// For runtime validation, use @/api/schemas.ts

import { Policy } from '@/api/adapter-types';
import type { Evidence } from '@/api/document-types';
import type { components } from './generated';

// ============================================================================
// Re-export Generated Types (Backend snake_case types)
// ============================================================================

// Infrastructure Types - From Generated
export type ComponentHealth = components['schemas']['ComponentHealth'];
export type ComponentStatus = components['schemas']['ComponentStatus'];
export type ErrorResponse = components['schemas']['ErrorResponse'];
export type FailureCode = components['schemas']['FailureCode'];
export type NodeHealth = components['schemas']['NodeHealth'];

// Health Types - From Generated
export type HealthResponse = components['schemas']['HealthResponse'];
export type BaseModelStatusResponse = components['schemas']['BaseModelStatusResponse'];
export type ModelLoadStatus = components['schemas']['ModelLoadStatus'];

// Inference Types - From Generated
export type InferRequest = components['schemas']['InferRequest'];

// Extended InferResponse with UI-computed fields
export type InferResponse = components['schemas']['InferResponse'] & {
  // Token counts (may be computed or from run_receipt)
  token_count?: number;
  // Backend alias (also available as backend_used)
  backend?: string;
};

export type BatchInferRequest = components['schemas']['BatchInferRequest'];
export type BatchInferResponse = components['schemas']['BatchInferResponse'];
export type BackendKind = components['schemas']['BackendKind'];
export type CoreMLMode = components['schemas']['CoreMLMode'];

// Routing Types - From Generated
export type RoutingDecision = components['schemas']['RoutingDecision'];
export type RoutingDecisionResponse = components['schemas']['RoutingDecisionResponse'];
export type RoutingPolicy = components['schemas']['RoutingPolicy'];

// Worker Types - From Generated
export type WorkerDetailResponse = components['schemas']['WorkerDetailResponse'];
export type WorkerResourceUsage = components['schemas']['WorkerResourceUsage'];
export type WorkerType = components['schemas']['WorkerType'];

// Tenant Types - From Generated
export type TenantResponse = components['schemas']['TenantResponse'];
export type TenantState = components['schemas']['TenantState'];

// Node Types - From Generated
export type NodeDetailResponse = components['schemas']['NodeDetailResponse'];
export type NodeState = components['schemas']['NodeState'];

// Telemetry Types - Legacy (kept for backward compatibility)
// Note: Telemetry types are not in generated.ts as they are internal-only

// ============================================================================
// Legacy Type Aliases (for backward compatibility)
// ============================================================================

/** @deprecated Use BackendKind instead */
export type BackendName = BackendKind;

/** @deprecated Use BackendKind instead */
export type BackendMode = 'real' | 'stub' | 'auto';

// ============================================================================
// Raw Backend Response Types (snake_case field names)
// MIGRATION NOTE: These should eventually be replaced with generated types
// For now, keeping for backward compatibility
// ============================================================================

export interface RawAdapterResponse {
  schema_version?: string;
  adapter_id?: string;
  id?: string;
  name: string;
  tenant_id?: string;
  hash_b3: string;
  rank: number;
  tier: string;
  lifecycle_state?: string;
  category?: 'code' | 'framework' | 'codebase' | 'ephemeral';
  scope?: 'global' | 'tenant' | 'repo' | 'commit' | 'project';
  framework?: string;
  description?: string;
  created_at: string;
  updated_at?: string;
  memory_bytes?: number;
  last_activated?: string;
  activation_count?: number;
  pinned?: boolean;
  current_state?: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  runtime_state?: string;
  kv_consistent?: boolean;
  kv_message?: string;
}

export interface RawInferResponse {
  schema_version: string;
  id: string;
  text: string;
  tokens_generated: number;
  latency_ms: number;
  adapters_used: string[];
  run_receipt?: unknown;
  citations?: unknown[];
  finish_reason?: 'stop' | 'length' | 'error' | 'budget' | 'repetition';
  stop_reason_code?: StopReasonCode;
  tokens?: number[];
  backend?: BackendKind;
  trace?: {
    latency_ms: number;
    steps?: Array<{ adapter: string; latency_ms: number; tokens: number }>;
    router_decisions?: Array<{ adapter: string; score: number }>;
    evidence_spans?: Array<{ text: string; relevance: number }>;
  };
  token_count?: number;
  model?: string;
  prompt_tokens?: number;
  error?: string;
  response?: string;
  unavailable_pinned_adapters?: string[];
  pinned_routing_fallback?: 'stack_only' | 'partial' | null;
  backend_used?: BackendKind | string;
  coreml_compute_preference?: string;
  coreml_compute_units?: string;
  coreml_gpu_used?: boolean | null;
  fallback_backend?: BackendKind | string;
  fallback_triggered?: boolean;
  determinism_mode_applied?: string;
  replay_guarantee?: string | null;
}

export interface RawRouterDecision {
  request_id: string;
  selected_adapters: string[];
  scores: Record<string, number>;
  timestamp: string;
  latency_ms: number;
  overhead_pct?: number;
  tau?: number;
  step?: number;
  stack_hash?: string;
  input_token_id?: number;
  entropy_floor?: number;
  entropy?: number;
  candidate_adapters?: unknown[]; // Backend uses candidate_adapters
  candidates?: unknown[]; // Some endpoints use candidates
  k_value?: number;
  router_latency_us?: number;
}

// ============================================================================
// Legacy Types (Preserved for backward compatibility)
// These are the original mixed snake_case/camelCase types.
// NEW CODE SHOULD USE domain-types.ts INSTEAD.
// ============================================================================

/** Generic paginated response wrapper used by list endpoints */
export interface PaginatedResponse<T> {
  schema_version?: string;
  data: T[];
  total: number;
  page: number;
  limit: number;
  pages: number;
}

export interface ModelArchitectureSummary {
  architecture?: string | null;
  num_layers?: number | null;
  hidden_size?: number | null;
  vocab_size?: number | null;
}

export interface ModelWithStatsResponse {
  id: string;
  name: string;
  hash_b3: string;
  config_hash_b3: string;
  tokenizer_hash_b3: string;
  format?: string | null;
  backend?: string | null;
  size_bytes?: number | null;
  import_status?: string | null;
  model_path?: string | null;
  capabilities?: string[] | null;
  quantization?: string | null;
  tenant_id?: string | null;
  adapter_count: number;
  training_job_count: number;
  imported_at?: string | null;
  updated_at?: string | null;
  architecture?: ModelArchitectureSummary | null;
}

export interface ModelListResponse {
  models: ModelWithStatsResponse[];
  total: number;
}

export interface UpdateMonitoringRuleRequest {
  name?: string;
  description?: string;
  enabled?: boolean;
  conditions?: Record<string, unknown>;
  actions?: Record<string, unknown>;
  severity?: 'low' | 'medium' | 'high' | 'critical';
}

// Prompt Orchestration types
/**
 * @deprecated Use OrchestrationConfig (defined around line 1968) instead - this is the legacy camelCase version
 */
export interface LegacyOrchestrationConfig {
  enabled: boolean;
  baseModelThreshold: number;
  adapterThreshold: number;
  analysisTimeout: number;
  cacheEnabled: boolean;
  cacheTtl: number;
  enableTelemetry: boolean;
  fallbackStrategy: 'base_only' | 'best_effort' | 'adaptive';
}

/**
 * @deprecated Use OrchestrationMetrics (defined around line 2009) instead - this is the legacy camelCase version
 */
export interface LegacyOrchestrationMetrics {
  totalRequests: number;
  baseModelOnly: number;
  adapterUsed: number;
  analysisTimeMs: number;
  cacheHits: number;
  cacheMisses: number;
  lastUpdated: string;
}

export interface PromptAnalysisResult {
  prompt: string;
  complexityScore: number;
  recommendedStrategy: 'base_model' | 'adapters' | 'mixed';
  analysisTimeMs: number;
  features: {
    language: string;
    frameworks: string[];
    symbols: number;
    tokens: number;
    verb: string;
  };
  timestamp: string;
}

// ============================================================================
// System Metrics (UI-specific aggregation, not in generated types)
// ============================================================================

export interface SystemMetrics {
  cpu_usage?: number;
  cpu_usage_percent?: number;
  cpu_cores?: number;
  memory_usage?: number;
  memory_used_gb?: number;
  memory_usage_pct?: number;
  memory_usage_percent?: number;
  memory_total_gb?: number;
  disk_usage?: number;
  disk_usage_percent?: number;
  network_rx?: number;
  network_tx?: number;
  network_rx_bytes?: number;
  network_tx_bytes?: number;
  gpu_utilization_percent?: number;
  gpu_memory_used_mb?: number;
  gpu_memory_total_mb?: number;
  timestamp?: string;
  adapter_count?: number;
  active_sessions?: number;
  tokens_per_second?: number;
  latency_p95_ms?: number;
  // Additional training-related metrics
  current_epoch?: number;
  total_epochs?: number;
  current_loss?: number;
  learning_rate?: number;
  // Additional hardware metrics
  cpu_temp_celsius?: number;
  gpu_temp_celsius?: number;
  gpu_power_watts?: number;
  disk_read_mbps?: number;
  disk_write_mbps?: number;
  disk_used_gb?: number;
  disk_total_gb?: number;
  network_rx_packets?: number;
  network_tx_packets?: number;
  // Performance metrics
  cache_hit_rate?: number;
  error_rate?: number;
}

// Alias for backward compatibility - BaseModelStatus now uses generated type
export interface BaseModelStatus extends BaseModelStatusResponse {
  schema_version?: string;
}

export interface ReplaySession {
  id: string;
  cpid: string;
  plan_id: string;
  snapshot_at: string;
  telemetry_bundle_ids: string[];
  manifest_hash_b3: string;
  policy_hash_b3: string;
  kernel_hash_b3?: string;
  // Optional fields for inference session replay
  prompt?: string;
  config?: {
    max_tokens?: number;
    temperature?: number;
    top_k?: number;
    top_p?: number;
    seed?: number;
    require_evidence?: boolean;
  };
}

export interface ReplayVerificationResponse {
  schema_version: string; // Required by backend API
  session_id: string;
  verified_at: string;
  signature_valid: boolean;
  hash_chain_valid: boolean;
  manifest_verified: boolean;
  policy_verified: boolean;
  kernel_verified: boolean;
  divergences: Array<{
    divergence_type: string;
    expected_hash: string;
    actual_hash: string;
    context: string;
  }>;
}

export type ReceiptReasonCode =
  | 'CONTEXT_MISMATCH'
  | 'TRACE_TAMPER'
  | 'OUTPUT_MISMATCH'
  | 'POLICY_MISMATCH'
  | 'BACKEND_MISMATCH'
  | 'SIGNATURE_INVALID'
  | 'MISSING_RECEIPT'
  | 'TRACE_NOT_FOUND';

export interface ReceiptDigestDiff {
  field: string;
  expected_hex: string;
  computed_hex: string;
  matches: boolean;
}

export interface ReceiptVerificationResult {
  trace_id: string;
  tenant_id?: string;
  source: 'trace' | 'bundle';
  pass: boolean;
  verified_at: string;
  reasons: ReceiptReasonCode[];
  mismatched_token?: number;
  context_digest: ReceiptDigestDiff;
  run_head_hash: ReceiptDigestDiff;
  output_digest: ReceiptDigestDiff;
  receipt_digest: ReceiptDigestDiff;
  signature_checked: boolean;
  signature_valid?: boolean | null;
}

// Node management types
export interface Node {
  id: string;
  hostname: string;
  status: 'healthy' | 'offline' | 'error';
  last_heartbeat?: string;
  memory_gb?: number;
  gpu_count?: number;
  agent_endpoint?: string;
  metal_family?: string;
}

export interface NodeDetailsResponse {
  schema_version: string; // Required by backend API
  id: string;
  hostname: string;
  status: string;
  memory_gb?: number;
  gpu_count?: number;
  last_seen_at?: string;
  workers: Array<{
    id: string;
    status: string;
    tenant_id: string;
    plan_id: string;
  }>;
  metal_family?: string;
  last_heartbeat?: string;
  gpu_type?: string;
}

export interface NodePingResponse {
  schema_version: string; // Required by backend API
  status: 'reachable' | 'unreachable' | 'timeout';
  latency_ms: number;
}

// Tenant types
export interface Tenant {
  id: string;
  name: string;
  uid?: number;
  gid?: number;
  isolation_level?: string;
  created_at?: string;
  status?: string;
  description?: string;
  adapters?: string[];
  users?: string[];
  itarCompliant?: boolean;
  itar_compliant?: boolean;
  data_classification?: string;
  policies?: string[];
  last_activity?: string;
}

// Dashboard configuration types
export interface DashboardWidgetConfig {
  id: string;
  user_id: string;
  widget_id: string;
  enabled: boolean;
  position: number;
  created_at: string;
  updated_at: string;
}

export interface WidgetConfigUpdate {
  widget_id: string;
  enabled: boolean;
  position: number;
}

// Tenant types
export interface CreateTenantRequest {
  name: string;
  uid?: number;
  gid?: number;
  isolation_metadata?: Record<string, unknown>;
  isolation_level?: string;
}

export interface RegisterNodeRequest {
  node_id: string;
  hostname: string;
  ip_address?: string;
  capabilities: {
    memory_gb?: number;
    agent_endpoint?: string;
    [key: string]: unknown;
  };
  metadata?: Record<string, unknown>;
  metal_family?: string;
}

// Worker types
export interface SpawnWorkerRequest {
  tenant_id: string;
  node_id: string;
  plan_id: string;
}

export interface WorkerResponse {
  schema_version: string; // Required by backend API
  id: string;  // alias
  worker_id: string;
  worker_type: string;
  status: 'starting' | 'running' | 'stopping' | 'stopped' | 'error';
  node_id: string;
  created_at: string;
  tenant_id?: string;
  memory_mb?: number;
  cpu_percent?: number;
  last_seen_at?: string;
  pid?: number;
  plan_id?: string;
  started_at?: string;
  manifest_hash?: string;  // Manifest hash for worker binding verification
  /** Current cache memory usage in MB */
  cache_used_mb?: number;
  /** Maximum cache memory budget in MB */
  cache_max_mb?: number;
  /** Number of pinned cache entries (cannot be evicted) */
  cache_pinned_entries?: number;
  /** Number of active cache entries (in-use, cannot be evicted) */
  cache_active_entries?: number;
}

export interface WorkerDetailsResponse extends WorkerResponse {
  pid?: number;
  memory_mb?: number;
  cpu_percent?: number;
  uptime_seconds?: number;
  last_heartbeat?: string;
  error?: string;
}

// Plan types
export interface Plan {
  id: string;
  name: string;
  description?: string;
  steps: PlanStep[];
  status: 'draft' | 'active' | 'archived';
  created_at: string;
  updated_at: string;
  cpid?: string;
  execution_count?: number;
  last_executed?: string;
  metallib_hash?: string;
}

export interface PlanStep {
  id: string;
  action: string;
  parameters: Record<string, unknown>;
  dependencies: string[];
  status: 'pending' | 'running' | 'completed' | 'failed';
}

export interface BuildPlanRequest {
  tenant_id: string;
  manifest_hash_b3: string;
}

export interface PlanComparisonResponse {
  schema_version: string; // Required by backend API
  plan_a: Plan;
  plan_b: Plan;
  plan_1?: Plan;  // alias for plan_a
  plan_2?: Plan;  // alias for plan_b
  differences: PlanDifference[];
  adapter_changes?: Array<{
    adapter_id: string;
    change_type: string;
    added?: boolean;
    removed?: boolean;
    modified?: boolean;
  }>;
  metallib_hash_changed?: boolean;
}

export interface PlanDifference {
  path: string;
  type: 'added' | 'removed' | 'modified';
  value_a?: unknown;
  value_b?: unknown;
}

// ============================================================================
// Router Types (UI-specific extensions, not in generated types)
// ============================================================================

export interface RouterParameters {
  k_sparse: number;
  tau: number;
  entropy_floor: number;
  gate_quant: string;
  sample_tokens_full: number;
  algorithm: string;
  warmup: boolean;
}

export interface RouterStackSummary {
  stack_id: string;
  version?: number;
  lifecycle_state?: string;
  adapter_ids: string[];
}

export interface RouterAdapterSummary {
  adapter_id: string;
  name?: string;
  tier?: string;
  category?: string;
  scope?: string;
  rank?: number;
  alpha?: number;
  in_default_stack: boolean;
}

export interface RouterConfigView {
  tenant_id: string;
  manifest_hash?: string;
  router: RouterParameters;
  routing_policy?: RoutingPolicy;
  stack?: RouterStackSummary;
  adapters: RouterAdapterSummary[];
}

// Promotion types
export interface PromotionRequest {
  tenant_id: string;
  cpid: string;
  plan_id: string;
}

export interface PromotionRecord {
  id: string;
  adapter_id: string;
  from_tier: string;
  to_tier: string;
  status: 'pending' | 'approved' | 'rejected' | 'completed';
  requested_by: string;
  approved_by?: string;
  created_at: string;
  completed_at?: string;
  gates: PromotionGate[];
  reason?: string;
  metadata?: Record<string, unknown>;
}

export interface PromotionGate {
  name: string;
  type: 'manual' | 'automated';
  status: 'pending' | 'passed' | 'failed';
  result?: Record<string, unknown>;
}

// Golden run promotion types
export interface PromotionResponse {
  schema_version: string; // Required by backend API
  request_id: string;
  run_id: string;
  target_stage: string;
  status: 'pending' | 'in_progress' | 'approved' | 'rejected';
  created_at: string;
}

export interface PromotionStatusResponse {
  schema_version: string; // Required by backend API
  run_id: string;
  current_stage: string;
  stages: PromotionStageStatus[];
  created_at: string;
  updated_at: string;
}

export interface PromotionStageStatus {
  id: string;
  name: string;
  description?: string;
  status: 'pending' | 'in_progress' | 'passed' | 'failed';
  approver?: string;
  approved_at?: string;
  notes?: string;
  gates?: GateStatus[];
}

export interface GateStatus {
  id: string;
  name: string;
  description?: string;
  status: 'pending' | 'passed' | 'failed';
  required: boolean;
  result?: Record<string, unknown>;
}

export interface ApproveResponse {
  schema_version: string; // Required by backend API
  run_id: string;
  stage_id: string;
  approved: boolean;
  approver: string;
  approved_at: string;
  notes?: string;
}

export interface RollbackResponse {
  schema_version: string; // Required by backend API
  stage: string;
  status: 'initiated' | 'completed' | 'failed';
  message: string;
  rolled_back_at: string;
}

// Git/Repository types
export interface Repository {
  id: string;
  name: string;
  url: string;
  branch: string;
  last_sync?: string;
  last_scan?: string;
  status: 'synced' | 'syncing' | 'error';
  commit_count?: number;
  default_branch?: string;
  size_kb?: number;
  url_is_fallback?: boolean;
}

export interface Commit {
  sha: string;
  message: string;
  author: string;
  timestamp: string;
  files_changed: number;
  diff_stats?: { additions: number; deletions: number; files_changed?: number };
}

export interface RegisterRepositoryRequest {
  repo_id: string;
  path: string;
}

export interface TriggerScanRequest {
  repository_id: string;
}

export interface TriggerScanResponse {
  schema_version: string; // Required by backend API
  job_id: string;
  repository_id: string;
  status: 'pending' | 'scanning' | 'completed' | 'failed';
  started_at: string;
}

export interface CommitDeltaRequest {
  repository_id: string;
  commit_sha: string;
  message?: string;
  files?: Array<{
    path: string;
    additions: number;
    deletions: number;
  }>;
}

export interface CommitDeltaResponse {
  schema_version: string; // Required by backend API
  delta_id: string;
  repository_id: string;
  commit_sha: string;
  status: 'pending' | 'processing' | 'completed' | 'failed';
  created_at: string;
  processed_at?: string;
}

// ============================================================================
// Additional Inference Types (from generated.ts)
// ============================================================================

export type StopReasonCode = components['schemas']['StopReasonCode'];
export type StopPolicySpec = components['schemas']['StopPolicySpec'];
export type RunReceipt = components['schemas']['RunReceipt'];
export type Citation = components['schemas']['Citation'];
export type CharRange = components['schemas']['CharRange'];
export type BoundingBox = components['schemas']['BoundingBox'];

// UI-only type (not exposed in API)
export type FusionInterval = {
  start_token: number;
  end_token: number;
  adapter_weights: Record<string, number>;
};

// Telemetry types
export interface TelemetryBundle {
  id: string;  // alias
  bundle_id: string;
  events: TelemetryEvent[];
  start_time: string;
  end_time: string;
  signature?: string;
  event_count?: number;
  size_bytes?: number;
  merkle_root?: string;
  cpid?: string;
  created_at?: string;
  tenant_id?: string;
  adapter_ids?: string[];
  manifest_hash_b3?: string;
  policy_hash_b3?: string;
}

export interface TelemetryEvent {
  event_id: string;
  event_type: string;
  timestamp: string;
  payload: Record<string, unknown>;
  metadata?: Record<string, unknown>;
  level?: string;
  trace_id?: string;
  user_id?: string;
  component?: string;
  tenant_id?: string;
  id?: string;
  message?: string;
}

export interface TelemetryQuery {
  event_types?: string[];
  start_time?: string;
  end_time?: string;
  limit?: number;
  offset?: number;
}

// ============================================================================
// Health & System Types (Additional)
// ============================================================================

export interface ReadyzCheck {
  ok: boolean;
  hint?: string;
}

export interface ReadyzChecks {
  db: ReadyzCheck;
  worker: ReadyzCheck;
  models_seeded: ReadyzCheck;
}

export interface ReadyzResponse {
  ready: boolean;
  checks: ReadyzChecks;
}

export interface BackendCapability {
  name: string;
  available: boolean;
  deterministic?: boolean;
  precision?: string[];
  notes?: string[];
}

export interface HardwareCapabilities {
  ane_available?: boolean;
  gpu_available?: boolean;
  gpu_type?: string;
  gpu_memory_gb?: number;
  cpu_model?: string;
  memory_gb?: number;
}

export interface BackendStatus {
  backend: BackendName;
  mode: BackendMode;
  status: 'healthy' | 'degraded' | 'unavailable';
  version?: string;
  deterministic?: boolean;
  supports_streaming?: boolean;
  supports_training?: boolean;
  last_checked?: string;
  warnings?: string[];
  errors?: string[];
  notes?: string[];
}

export interface BackendStatusResponse {
  schema_version: string;
  status: BackendStatus;
  capabilities?: BackendCapability[];
  hardware?: HardwareCapabilities;
}

export interface BackendListResponse {
  schema_version: string;
  backends: BackendStatus[];
  default_backend?: BackendName;
}

export interface BackendCapabilitiesResponse {
  schema_version: string;
  hardware: HardwareCapabilities;
  backends: Array<{
    backend: BackendName;
    capabilities: BackendCapability[];
  }>;
}

export interface MetaResponse {
  schema_version: string; // Required by backend API
  version: string;
  build_date: string;
  git_commit: string;
  features: string[];
  last_updated?: string;
  uptime?: number;
  build_hash?: string;
  /** Runtime environment: "dev", "staging", or "prod" */
  environment?: string;
  /** Whether production mode is enabled in config */
  production_mode?: boolean;
  /** Whether dev login bypass is enabled */
  dev_login_enabled?: boolean;
}

export interface JourneyState {
  state: string;
  timestamp: string | number;
  details: Record<string, unknown>;
}

export interface JourneyResponse {
  schema_version: string; // Required by backend API
  journey_id: string;
  steps: JourneyStep[];
  current_step: number;
  completed: boolean;
  states: JourneyState[];
  id?: string;
  journey_type?: string;
  created_at?: string;
}

export interface JourneyStep {
  id: string;
  name: string;
  status: 'pending' | 'in_progress' | 'completed' | 'skipped';
  metadata?: Record<string, unknown>;
}

export interface ProcessServiceStatus {
  name: string;
  status: 'running' | 'stopped' | 'error';
  pid?: number;
  uptime?: number;
  memory_mb?: number;
  health_check_url?: string;
  last_check?: string;
}

export interface Alert {
  id: string;
  severity: 'critical' | 'high' | 'medium' | 'low' | 'info' | 'warning' | 'error';
  message: string;
  timestamp: string;
  acknowledged: boolean;
  acknowledged_by?: string;
  acknowledged_at?: string;
  source?: string;
  created_at?: string;
  resolved?: boolean;
  resolved_at?: string;
  title?: string;
  triggered_at?: string;
  threshold_value?: number;
  metric_value?: number;
  rule_name?: string;
  status?: string;
}

/** Alert for when model cache budget is exceeded */
export interface CacheBudgetAlert {
  worker_id: string;
  tenant_id: string;
  severity: 'critical' | 'high' | 'medium';
  /** Memory needed in MB */
  needed_mb: number;
  /** Memory freed during eviction in MB */
  freed_mb: number;
  /** Maximum cache budget in MB */
  max_mb: number;
  /** Number of pinned entries blocking eviction */
  pinned_entries: number;
  /** Number of active entries blocking eviction */
  active_entries: number;
  /** ISO-8601 timestamp */
  timestamp: string;
  /** Model key that triggered the alert */
  model_key?: string;
}

/** Worker cache health status derived from metrics */
export interface WorkerCacheHealth {
  worker_id: string;
  /** Usage as percentage (0-100) */
  utilization_pct: number;
  /** Health status based on utilization thresholds */
  status: 'healthy' | 'warning' | 'critical';
  cache_used_mb: number;
  cache_max_mb: number;
  cache_pinned_entries: number;
  cache_active_entries: number;
}

export interface ModelStatusResponse {
  schema_version: string; // Required by backend API
  model_id: string;
  status: 'no-model' | 'loading' | 'ready' | 'unloading' | 'error' | 'checking';
  memory_usage_mb?: number;
  last_inference?: string;
  error?: string;
  is_loaded?: boolean;
  error_message?: string;
  model_name?: string;
  model_path?: string | null;
}

// Auth config types
export interface UpdateAuthConfigRequest {
  jwt_mode?: string;
  token_expiry_hours?: number;
  require_mfa?: boolean;
}

// Policy pack types
export interface PolicyPackResponse {
  schema_version: string; // Required by backend API
  pack_id: string;
  name: string;
  version: string;
  policies: Policy[];
  enabled: boolean;
  created_at: string;
}

export interface SignPolicyResponse {
  schema_version: string; // Required by backend API
  policy_id: string;
  signature: string;
  signed_at: string;
  signed_by: string;
  cpid?: string;
}

export interface PolicyComparisonResponse {
  cpid_1: string;
  cpid_2: string;
  differences: string[];
  identical: boolean;
}

export interface ExportPolicyResponse {
  schema_version: string; // Required by backend API
  policy_id: string;
  format: 'json' | 'yaml';
  content: string;
}

// Model import/validation types
export interface ImportModelRequest {
  source?: 'huggingface' | 'local' | 'url';
  model_id?: string;
  name?: string;
  quantization?: string;
  model_name?: string;
  weights_path?: string;
  config_path?: string;
  tokenizer_path?: string;
  tokenizer_config_path?: string;
  metadata?: Record<string, unknown>;
}

export interface ImportModelResponse {
  schema_version: string; // Required by backend API
  import_id: string;
  model_id: string;
  status: 'pending' | 'downloading' | 'converting' | 'completed' | 'failed';
  progress?: number;
  error?: string;
}

export interface ModelValidationResponse {
  model_id: string;
  status: string; // "ready" | "needs_setup" | "invalid"
  valid: boolean;
  can_load: boolean;
  reason?: string;
  issues: Array<{ type: string; message: string }>;
  errors?: string[]; // Legacy field for backwards compatibility
  download_commands?: string[];
}

export interface ModelDownloadArtifact {
  artifact: string;
  filename: string;
  download_url: string;
  size_bytes?: number;
}

export interface ModelDownloadResponse {
  schema_version: string; // Required by backend API
  model_id: string;
  download_url: string;
  expires_at: string;
  size_bytes: number;
  artifacts?: ModelDownloadArtifact[];
}

/**
 * Response from starting a model download job
 */
export interface DownloadJobResponse {
  job_id: string;
  model_id: string;
  status: 'queued' | 'downloading' | 'completed' | 'failed';
  progress_percent: number;
  downloaded_bytes: number;
  total_bytes: number;
  error?: string;
}

export interface AllModelsStatusResponse {
  schema_version: string; // Required by backend API
  models: BaseModelStatus[]; // Uses BaseModelStatus which includes model_path
  total_memory_mb: number;
  available_memory_mb?: number;
  active_model_count: number;
}

// Routing debug types
export interface RoutingDebugRequest {
  prompt: string;
  top_k?: number;
  include_scores?: boolean;
}

export interface RoutingDebugResponse {
  schema_version: string; // Required by backend API
  selected_adapters: string[];
  all_scores: Record<string, number>;
  gate_values: number[];
  decision_time_ms: number;
}

// Scan types
export interface ScanStatusResponse {
  schema_version: string; // Required by backend API
  scan_id: string;
  status: 'pending' | 'scanning' | 'completed' | 'failed';
  progress?: number;
  results?: ScanResult[];
}

export interface ScanResult {
  file_path: string;
  issues: Array<{ severity: string; message: string; line?: number }>;
}

// Code quality types
export interface CommitDiff {
  sha: string;
  files: Array<{
    path: string;
    additions: number;
    deletions: number;
    patch?: string;
  }>;
  stats?: { additions: number; deletions: number; files: number; files_changed?: number; insertions?: number };
  diff?: string;
}

export interface QualityMetrics {
  score: number;
  complexity: number;
  coverage?: number;
  issues: number;
  breakdown: Record<string, number>;
  cr?: number;
  hlr?: number;
  ecs5?: number;
  arr?: number;
}

// ============================================================================
// Tenant Usage Response (Additional, not in generated)
// ============================================================================

export interface TenantUsageResponse {
  schema_version: string; // Required by backend API
  tenant_id: string;
  period: string;
  inference_count: number;
  inference_count_24h?: number;
  tokens_processed: number;
  training_jobs: number;
  storage_mb: number;
  memory_used_gb?: number;
  memory_total_gb?: number;
  gpu_usage_pct?: number;
  cpu_usage_pct?: number;
  active_adapters_count?: number;
}

export interface TenantStorageUsageResponse {
  schema_version?: string;
  tenant_id: string;
  dataset_bytes: number;
  artifact_bytes: number;
  dataset_versions: number;
  adapter_versions: number;
  soft_limit_bytes: number;
  hard_limit_bytes: number;
  soft_exceeded: boolean;
  hard_exceeded: boolean;
}

export interface AssignPoliciesResponse {
  schema_version: string; // Required by backend API
  tenant_id: string;
  assigned_policies: string[];
  updated_at: string;
}

export interface AssignAdaptersResponse {
  schema_version: string; // Required by backend API
  tenant_id: string;
  assigned_adapters: string[];
  updated_at: string;
}

// Promotion response types
export interface DryRunPromotionResponse {
  schema_version: string; // Required by backend API
  would_succeed: boolean;
  gates_status: PromotionGate[];
  warnings: string[];
  estimated_duration_ms?: number;
}

export interface PromotionHistoryEntry {
  id: string;
  adapter_id: string;
  from_tier: string;
  to_tier: string;
  status: string;
  requested_by: string;
  created_at: string;
  cpid?: string;
  promoted_by?: string;
  promoted_at?: string;
}

// Telemetry response types
export interface ExportTelemetryBundleResponse {
  schema_version: string; // Required by backend API
  bundle_id: string;
  download_url: string;
  expires_at: string;
  format: 'json' | 'parquet';
  size_bytes?: number;
  events_count?: number;
}

export interface VerifyBundleSignatureResponse {
  schema_version: string; // Required by backend API
  bundle_id: string;
  valid: boolean;
  signer?: string;
  signed_by?: string;
  signed_at?: string;
  error?: string;
  verification_error?: string;
  signature?: string;
}

export interface PurgeOldBundlesResponse {
  schema_version: string; // Required by backend API
  deleted_count: number;
  freed_bytes: number;
  oldest_remaining?: string;
}

// Repository response types
export interface RepositoryReportResponse {
  schema_version: string; // Required by backend API
  repository_id: string;
  commit_count: number;
  branch_count: number;
  last_activity: string;
  contributors: string[];
  total_files?: number;
  total_lines?: number;
  languages?: Record<string, number>;
  ephemeral_adapters_count?: number;
}

export interface RegisterGitRepositoryResponse {
  schema_version: string; // Required by backend API
  repository_id: string;
  name: string;
  url: string;
  status: 'pending' | 'syncing' | 'synced' | 'error';
  analysis?: { files_count: number; languages: string[]; git_info?: Record<string, unknown>; frameworks?: string[] };
}

export interface Notification {
  id: string;
  type: 'system_alert' | 'user_message' | 'activity_event' | 'resource_share' | 'mention' | 'alert' | 'message' | 'activity' | 'system';
  title: string;
  message: string;
  timestamp: string;
  created_at?: string;
  read_at?: string;
  action_url?: string;
  metadata?: Record<string, unknown>;
  workspace_id?: string;
  target_id?: string;
  actor_id?: string;
  target_type?: string;
  priority?: 'low' | 'normal' | 'high';
  expires_at?: string;
  content?: string;
}

export interface NotificationSummary {
  total_count: number;
  unread_count: number;
  by_type?: Record<string, number>;
}

export interface UnifiedTelemetryEvent extends TelemetryEvent {
  source?: string;
  correlation_id?: string;
  id?: string;
  message?: string;
}

export interface Trace {
  trace_id: string;
  root_span_id: string;
  spans: Array<{
    span_id: string;
    name: string;
    start_time: string;
    end_time: string;
    attributes?: Record<string, unknown>;
    // Required fields for TraceTimeline component
    trace_id: string;
    parent_id: string;
    start_ns: number;
    end_ns: number;
    status: string;
  }>;
}

// Dev-only contract samples served by /v1/dev/contracts
export interface ContractSamplesResponse {
  inference: InferResponse;
  trace: Trace;
  evidence: Evidence[];
}

// Trace v1 response for per-token inspection
export interface TraceResponseV1 {
  trace_id: string;
  context_digest: string;
  policy_digest: string;
  backend_id: string;
  kernel_version_id: string;
  tokens: Array<{
    token_index: number;
    token_id?: string;
    selected_adapter_ids: string[];
    gates_q15: number[];
    decision_hash: string;
    policy_mask_digest: string;
    fusion_interval_id?: string;
    fused_weight_hash?: string;
  }>;
}

export interface TroubleshootingResult {
  issue_id: string;
  diagnosis: string;
  recommendations: string[];
  severity: 'low' | 'medium' | 'high';
  resolved?: boolean;
  success?: boolean;
  step_id?: string;
  output?: string;
}

export interface Tutorial {
  id: string;
  title: string;
  description: string;
  steps: Array<{ title: string; content: string; id?: string; position?: number; target_selector?: string }>;
  completed?: boolean;
  completed_at?: string;
  dismissed?: boolean;
  dismissed_at?: string;
  dismissible?: boolean;
  trigger?: string;
}

export interface UpdateDashboardConfigResponse {
  schema_version: string; // Required by backend API
  success: boolean;
  config_id: string;
  updated_at: string;
}

export interface ProcessLog {
  id: string;
  process_id: string;
  worker_id: string;
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
  metadata?: Record<string, unknown>;
}

export interface ProcessCrash {
  id: string;
  worker_id: string;
  crash_type: string;
  crash_timestamp: string;
  exit_code?: number;
  signal?: string;
  stack_trace?: string;
  memory_snapshot_json?: string;
  recovery_action?: string;
  recovered_at?: string;
  // Legacy fields for backwards compatibility
  process_id?: string;
  timestamp?: string;
}

export interface ProcessCrashDump {
  id: string;
  process_id: string;
  worker_id: string;
  crash_type: string;
  timestamp: string;
  exit_code?: number;
  signal?: string;
  stack_trace?: string;
}

export interface MetricsSnapshotResponse {
  schema_version: string; // Required by backend API
  timestamp: string;
  metrics: Record<string, number>;
  labels?: Record<string, string>;
  counters?: Record<string, number>;
  gauges?: Record<string, number>;
  histograms?: Record<string, unknown>;
}

export interface MetricsSeriesResponse {
  schema_version: string; // Required by backend API
  metric_name: string;
  data_points: Array<{ timestamp: string; value: number }>;
  aggregation?: string;
}

export interface HealthMetric {
  name: string;
  value: number;
  unit?: string;
  threshold?: number;
  status: 'healthy' | 'warning' | 'critical';
}

export interface DebugSession {
  session_id: string;
  process_id: string;
  started_at: string;
  ended_at?: string;
  breakpoints?: Array<{ file: string; line: number }>;
  config?: Record<string, unknown>;
  created_at?: string;
}

export interface DashboardConfig {
  id: string;
  name: string;
  layout: Record<string, unknown>;
  widgets: string[];
  created_at: string;
  updated_at: string;
}

export interface ResetDashboardConfigResponse {
  schema_version: string; // Required by backend API
  success: boolean;
  config_id: string;
  reset_at: string;
}

export interface ComplianceAuditResponse {
  schema_version: string; // Required by backend API
  audit_id: string;
  status: 'passed' | 'failed' | 'warning';
  findings: Array<{ rule: string; status: string; message: string }>;
  generated_at: string;
  controls?: Array<{ name: string; status: string; message?: string }>;
  violations?: Array<{ rule: string; message: string; severity?: string }>;
}

export interface UpdateDashboardConfigRequest {
  config_id?: string;
  layout?: Record<string, unknown>;
  widgets?: WidgetConfigUpdate[];
  name?: string;
}

export interface TroubleshootingStep {
  worker_id: string;
  step_name: string;
  step_type: string;
  command?: string;
  // Legacy fields for backwards compatibility
  step_id?: string;
  title?: string;
  description?: string;
  action?: string;
  expected_result?: string;
  parameters?: Record<string, unknown>;
}

export interface RoutingDecisionFilters {
  adapter_id?: string;
  stack_id?: string;
  since?: string;
  until?: string;
  min_entropy?: number;
  max_overhead_pct?: number;
  limit?: number;
  offset?: number;
  tenant_id?: string;
  anomalies_only?: boolean;
  request_id?: string;
   source_type?: string;
}

export interface AdapterFired {
  adapter_idx: number;
  gate_value: number;
  selected: boolean;
}

export interface SessionStep {
  step: number;
  timestamp: string;
  input_token_id?: number;
  adapters_fired: AdapterFired[];
  entropy: number;
  tau: number;
}

export interface SessionRouterViewResponse {
  request_id: string;
  stack_id?: string;
  stack_hash?: string;
  steps: SessionStep[];
  total_steps: number;
}

export interface DeterminismStatusResponse {
  last_run: string | null;
  result: 'pass' | 'fail' | null;
  runs?: number;
  divergences?: number;
}

export interface AdapterQuarantineStatusResponse {
  quarantined_count: number;
  quarantined_adapters: Array<{
    id: string;
    reason: string;
    created_at: string;
  }>;
  in_active_stacks: boolean;
}

export interface CapacityLimits {
  models_per_worker?: number;
  models_per_tenant?: number;
  concurrent_requests?: number;
}

export interface CapacityUsage {
  models_loaded: number;
  adapters_loaded: number;
  active_requests: number;
  ram_used_bytes: number;
  vram_used_bytes: number;
  ram_headroom_pct: number;
  vram_headroom_pct: number;
}

export interface CapacityResponse {
  total_ram_bytes: number;
  total_vram_bytes: number;
  limits: CapacityLimits;
  usage: CapacityUsage;
  node_health: NodeHealth;
}

export interface ResolveAlertRequest {
  alert_id: string;
  resolution?: string;
  resolved_by?: string;
}

export interface TransformedRoutingDecision {
  id: string;
  transformed?: boolean;
  display_adapters?: string[];
  request_id: string;
  selected_adapters: string[];
  scores: Record<string, number>;
  timestamp: string;
  latency_ms: number;
  overhead_pct?: number;
  tau?: number;
  step?: number;
  stack_hash?: string;
  input_token_id?: number;
  entropy_floor?: number;
  entropy?: number;
  k_value?: number;
  router_latency_us?: number;
  candidates: RouterCandidateInfo[];
  items?: TransformedRoutingDecision[];
}

export interface DebugSessionConfig {
  session_type: string;
  target_process?: string;
  breakpoints?: Array<{ file: string; line: number }>;
  max_duration_ms?: number;
}

export interface ProcessDebugSession extends DebugSession {
  id: string;
  worker_id: string;
  status: 'active' | 'paused' | 'ended';
  session_type?: string;
  config_json?: string;
}

export interface ProcessLogFilters {
  process_id?: string;
  level?: string;
  start_time?: string;
  end_time?: string;
  limit?: number;
}

export interface AlertFilters {
  severity?: 'critical' | 'high' | 'medium' | 'low' | 'info' | 'warning' | 'error';
  status?: string;
  start_time?: string;
  end_time?: string;
  limit?: number;
  tenant_id?: string;
  worker_id?: string;
  sort?: string;
}

export interface AcknowledgeAlertRequest {
  alert_id: string;
  acknowledged_by?: string;
  notes?: string;
}

export interface LanguageInfo {
  name: string;
  version?: string;
  extensions: string[];
}

export interface FrameworkInfo {
  name: string;
  version?: string;
  language?: string;
}

export interface CreateReplaySessionRequest {
  bundle_id?: string;
  telemetry_bundle_ids?: string[];
  name?: string;
  config?: Record<string, unknown>;
  tenant_id?: string;
  cpid?: string;
  plan_id?: string;
}

// Inference session for tracking inference history
export interface InferenceSession {
  stack_id?: string;
  stack_name?: string;
  id: string;
  created_at: string;
  prompt: string;
  request: InferRequest;
  response: InferResponse;
  status: 'pending' | 'running' | 'completed' | 'failed';
}

// Inference configuration type
export interface InferenceConfig extends InferRequest {
  id: string;
}

// Compliance and audit types
export interface ComplianceControl {
  id: string;
  control_id?: string;
  control_name?: string;
  name: string;
  status: string;
  category?: string;
  message?: string;
  last_checked?: string;
  details?: Record<string, unknown>;
  evidence?: string;
  findings?: string[];
}

export interface PolicyViolationRecord {
  id: string;
  policy_id: string;
  rule: string;
  message: string;
  reason?: string;
  severity?: 'low' | 'medium' | 'high' | 'critical';
  timestamp: string;
  created_at?: string;
  adapter_id?: string;
  tenant_id?: string;
  cpid?: string;
  violation_type?: string;
  metadata?: string;
  resolved?: boolean;
  resolved_at?: string;
  released?: boolean;
}

// Policy pack configuration
export interface PolicyPackConfig {
  pack_id: string;
  name: string;
  enabled: boolean;
  config: Record<string, unknown>;
  version?: string;
  description?: string;
}

// Router types
export interface FeatureVector {
  adapter_id: string;
  features: number[];
  normalized?: boolean;
  dimension?: number;
}

export interface RouterCandidateInfo {
  adapter_id: string;
  adapter_idx: number;
  gate_q15: number;
  gate_float: number;
  raw_score: number;
  selected: boolean;
  rank?: number;
  score?: number;
}

// Extended router decision with additional fields needed by UI
// Note: We can't extend RoutingDecision directly because candidates type conflicts
// (RoutingDecision has candidates?: string[], we need RouterCandidateInfo[])
export interface ExtendedRouterDecision {
  request_id: string;
  selected_adapters: string[];
  scores: Record<string, number>;
  timestamp: string;
  latency_ms: number;
  overhead_pct?: number;
  tau?: number;
  step?: number;
  stack_hash?: string;
  input_token_id?: number;
  entropy_floor?: number;
  entropy?: number;
  interval_id?: string;
  allowed_mask?: boolean[];
  candidates?: RouterCandidateInfo[]; // Extended: detailed candidate info instead of string[]
  k_value?: number; // Extended: K-sparse value
  adapter_map?: Map<number, string>; // Extended: For debugging: maps adapter_idx to adapter_id
  // Pinned adapter fallback tracking
  unavailable_pinned_adapters?: string[];
  pinned_routing_fallback?: 'stack_only' | 'partial' | null;
  policy_mask_digest?: string;
  policy_overrides_applied?: {
    allow_list: boolean;
    deny_list: boolean;
    trust_state: boolean;
  };
}

// Audit log entry
export interface AuditLog {
  id: string;
  user_id: string;
  action: string;
  resource: string;
  resource_id?: string;
  status: 'success' | 'failure' | 'error';
  timestamp: string;
  ip_address?: string;
  user_agent?: string;
  details?: Record<string, unknown>;
  tenant_id?: string;
  session_id?: string;
  user_role?: string;
  error_message?: string;
  metadata_json?: string;
}

export interface AuditLogResponse {
  id: string;
  timestamp: string;
  user_id: string;
  user_role: string;
  tenant_id: string;
  action: string;
  resource_type: string;
  resource_id?: string;
  status: 'success' | 'failure' | 'error';
  error_message?: string;
  ip_address?: string;
  metadata_json?: string;
}

export interface AuditLogsResponse {
  logs: AuditLogResponse[];
  total: number;
  limit: number;
  offset: number;
}

// Type alias for backward compatibility - matches AuditLogResponse structure
export type AuditLogEntry = AuditLogResponse;

export interface AuditLogFilters {
  action?: string;
  user_id?: string;
  resource?: string;
  status?: string;
  start_time?: string;
  end_time?: string;
  limit?: number;
  offset?: number;
  tenant_id?: string;
}

export interface PolicyAuditDecision {
  id: string;
  tenant_id: string;
  policy_pack_id: string;
  hook: string;
  decision: string;
  reason?: string | null;
  request_id?: string | null;
  user_id?: string | null;
  resource_type?: string | null;
  resource_id?: string | null;
  metadata_json?: string | null;
  timestamp: string;
  entry_hash: string;
  previous_hash?: string | null;
  chain_sequence: number;
}

export interface PolicyAuditBrokenLink {
  sequence: number;
  entry_id: string;
  expected_hash: string;
  actual_hash: string;
}

export interface PolicyAuditChainVerification {
  valid: boolean;
  total_entries: number;
  verified_entries: number;
  broken_links: PolicyAuditBrokenLink[];
  tenant_id?: string | null;
}

export interface DivergeAuditChainResponse {
  status: string;
  tenant_id: string;
  corrupted_entry_id: string;
  corrupted_hash: string;
  chain_sequence: number;
}

// Isolation test types
export interface IsolationTestScenario {
  id: string;
  name: string;
  description: string;
  category: 'tenant' | 'memory' | 'network' | 'filesystem';
}

export interface IsolationTestResult {
  scenario_id: string;
  passed: boolean;
  message: string;
  duration_ms: number;
  timestamp: string;
  details?: Record<string, unknown>;
}

// Threat monitoring types
export interface AnomalyDetectionStatus {
  enabled: boolean;
  last_scan: string;
  anomalies_detected: number;
  model_version: string;
}

export interface AccessPattern {
  hour: number;
  count: number;
  anomaly_score: number;
}

// Inference trace for observability
export interface InferenceTrace {
  latency_ms: number;
  router_decisions?: Array<{
    adapter: string;
    score: number;
    adapters?: string[];
    latency_ms?: number;
    // Per-token routing decision properties
    step?: number;
    token_idx?: number;
    input_token_id?: number;
    entropy?: number;
    tau?: number;
    entropy_floor?: number;
    candidate_adapters?: Array<{
      adapter_idx: number;
      raw_score: number;
      gate_q15: number;
    }> | string[];
    gates?: number[];
    stack_hash?: string;
    interval_id?: string;
  }>;
  evidence_spans?: Array<{
    text: string;
    relevance: number;
    source?: string;
    // Evidence span identification
    doc_id?: string;
    span_hash?: string;
  }>;
  steps?: Array<{
    adapter: string;
    latency_ms: number;
    tokens: number;
  }>;
}

// Prompt Orchestration types
export interface OrchestrationConfig {
  enabled: boolean;
  routing_strategy: 'entropy' | 'round_robin' | 'load_balanced' | 'weighted';
  default_adapter_stack?: string;
  max_adapters_per_request: number;
  timeout_ms: number;
  fallback_enabled: boolean;
  fallback_adapter?: string;
  entropy_threshold?: number;
  confidence_threshold?: number;
  cache_enabled: boolean;
  cache_ttl_seconds: number;
  telemetry_enabled: boolean;
  custom_rules?: OrchestrationRule[];
}

export interface OrchestrationRule {
  id: string;
  name: string;
  condition: string;
  adapter_stack: string;
  priority: number;
  enabled: boolean;
}

export interface PromptAnalysis {
  prompt_hash: string;
  detected_intent: string;
  confidence: number;
  suggested_adapters: string[];
  complexity_score: number;
  token_count: number;
  domain_classification?: string;
  language_detected?: string;
  routing_recommendation: {
    strategy: string;
    adapter_stack: string;
    reasoning: string;
  };
}

export interface OrchestrationMetrics {
  total_requests: number;
  successful_requests: number;
  failed_requests: number;
  average_latency_ms: number;
  p50_latency_ms: number;
  p95_latency_ms: number;
  p99_latency_ms: number;
  cache_hit_rate: number;
  adapter_usage: Record<string, number>;
  routing_decisions_by_strategy: Record<string, number>;
  errors_by_type: Record<string, number>;
  last_updated: string;
}

// System Health Response (all components)
export interface SystemHealthResponse {
  schema_version: string; // Required by backend API
  status: 'healthy' | 'degraded' | 'unhealthy';
  version: string;
  uptime_seconds: number;
  timestamp: string;
  components: Record<string, ComponentHealth>;
}

/**
 * Per-dependency health status with structured failure tracking
 */
export interface DependencyHealth {
  /** Dependency name (matches component names: db, router, workers, etc.) */
  name: string;
  /** Current status */
  status: 'healthy' | 'degraded' | 'unhealthy';
  /** Failure code if this dependency has failed */
  failure_code?: FailureCode;
  /** Human-readable message */
  message?: string;
  /** Whether this dependency is being retried */
  retrying?: boolean;
  /** Number of retry attempts */
  retry_count?: number;
  /** Timestamp of last check (milliseconds since epoch) */
  last_checked?: number;
}

/**
 * System-wide readiness response from /system/ready endpoint
 * Provides detailed boot status and structured failure codes
 */
export interface SystemReadyResponse {
  /** Whether the system is ready to accept requests */
  ready: boolean;
  /** Overall system status */
  overall_status: 'healthy' | 'degraded' | 'unhealthy';
  /** Individual component health status */
  components: ComponentHealth[];
  /** Milliseconds since boot started */
  boot_elapsed_ms?: number;
  /** List of critical components that are degraded */
  critical_degraded?: string[];
  /** List of non-critical components that are degraded */
  non_critical_degraded?: string[];
  /** Whether the system is in maintenance mode */
  maintenance?: boolean;
  /** Human-readable reason for current state */
  reason?: string;

  // Boot error taxonomy fields (added Dec 2024)
  /** Current boot state (e.g., "Starting", "LoadingModels", "Ready") */
  state?: string;
  /** Structured failure code if boot failed */
  reason_code?: FailureCode;
  /** Timestamp when current state started (milliseconds since epoch) */
  since?: number;
  /** Detailed error message for last failure */
  last_error?: string;
  /** Whether automatic retry is in progress */
  retrying?: boolean;
  /** Per-dependency health status with failure codes */
  dependency_status?: DependencyHealth[];
}

// Anomaly Response
export interface Anomaly {
  id: string;
  type: string;
  severity: 'critical' | 'high' | 'medium' | 'low' | 'info';
  status: 'open' | 'acknowledged' | 'resolved' | 'investigating';
  message: string;
  detected_at: string;
  resolved_at?: string;
  component?: string;
  metric_name?: string;
  metric_value?: number;
  threshold?: number;
  anomaly_score?: number;
  evidence?: string;
  tags?: string[];
}

// Update Anomaly Status Request
export interface UpdateAnomalyStatusRequest {
  status: 'open' | 'acknowledged' | 'resolved' | 'investigating';
  notes?: string;
  assigned_to?: string;
  tags?: string[];
}

// Worker health and incident types
export interface WorkerIncident {
  id: string;
  worker_id: string;
  tenant_id: string;
  incident_type: 'fatal' | 'crash' | 'hung' | 'degraded' | 'recovered';
  reason: string;
  backtrace_snippet?: string;
  latency_at_incident_ms?: number;
  created_at: string;
}

export interface WorkerHealthSummary {
  worker_id: string;
  health_status: 'healthy' | 'degraded' | 'crashed' | 'unknown';
  avg_latency_ms: number;
  total_requests: number;
  total_failures: number;
}
