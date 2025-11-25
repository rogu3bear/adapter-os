// General API response and utility types
// Extracted from types.ts for better organization
//
// 【2025-01-20†rectification†api_types】

import { Policy } from './adapter-types';

export interface OpenAIModelInfo {
  id: string;
  object: string;
  created: number;
  owned_by: string;
}

export interface OpenAIModelsListResponse {
  object: string;
  data: OpenAIModelInfo[];
}

export interface UpdateMonitoringRuleRequest {
  name?: string;
  description?: string;
  enabled?: boolean;
  conditions?: any;
  actions?: any;
  severity?: 'low' | 'medium' | 'high' | 'critical';
}

// Prompt Orchestration types
export interface OrchestrationConfig {
  enabled: boolean;
  baseModelThreshold: number;
  adapterThreshold: number;
  analysisTimeout: number;
  cacheEnabled: boolean;
  cacheTtl: number;
  enableTelemetry: boolean;
  fallbackStrategy: 'base_only' | 'best_effort' | 'adaptive';
}

export interface OrchestrationMetrics {
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

export interface ErrorResponse {
  schema_version: string; // Required by backend API
  error: string;
  code?: string;
  details?: string;
  timestamp?: string;
}

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

export interface BaseModelStatus {
  schema_version: string; // Required for ModelStatusResponse compatibility
  status: 'loading' | 'error' | 'unloaded' | 'ready' | 'loaded' | 'unloading';
  model_name: string;
  model_id: string;
  memory_usage_mb?: number;
  loaded_at?: string;
  error_message?: string;
  is_loaded?: boolean;
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

// Routing types
export interface RoutingDecision {
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
  candidates?: string[];
}

export interface RouterConfig {
  k_sparse: number;
  gate_quant: string;
  entropy_floor: number;
  sample_tokens_full: number;
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

// Inference types
export interface InferRequest {
  prompt: string;
  model?: string;
  max_tokens?: number;
  temperature?: number;
  top_p?: number;
  top_k?: number;
  stream?: boolean;
  adapter_stack?: string[];
  stack_id?: string;
  seed?: number;
  require_evidence?: boolean;
  adapters?: string[];
}

export interface InferResponse {
  schema_version: string; // Required by backend API
  id: string;
  text: string;
  tokens_generated: number;
  latency_ms: number;
  adapters_used: string[];
  finish_reason: 'stop' | 'length' | 'error';
  trace?: {
    latency_ms: number;
    steps?: Array<{ adapter: string; latency_ms: number; tokens: number }>;
    router_decisions?: Array<{ adapter: string; score: number }>;
    evidence_spans?: Array<{ text: string; relevance: number }>;
  };
  /** @deprecated Use token_count instead */
  tokens?: number;
  token_count?: number;
  model?: string;
  prompt_tokens?: number;
  error?: string;
  response?: string;
}

export interface BatchInferRequest {
  prompts?: string[];
  model?: string;
  max_tokens?: number;
  temperature?: number;
  adapter_stack?: string[];
  requests?: InferRequest[];
}

export interface BatchInferResponse {
  schema_version: string; // Required by backend API
  results: InferResponse[];
  responses: InferResponse[];  // alias for results
  total_tokens: number;
  total_latency_ms: number;
}

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

// Health & System types
export interface HealthResponse {
  schema_version: string; // Required by backend API
  status: 'healthy' | 'degraded' | 'unhealthy';
  version: string;
  uptime_seconds: number;
  components: Record<string, ComponentHealth>;
  checks?: Record<string, boolean>;
}

export interface ComponentHealth {
  status: 'healthy' | 'degraded' | 'unhealthy';
  message?: string;
  last_check: string;
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

export interface ServiceStatus {
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

export interface ModelStatusResponse {
  schema_version: string; // Required by backend API
  model_id: string;
  status: 'loading' | 'ready' | 'error' | 'unloaded' | 'loaded' | 'unloading';
  memory_usage_mb?: number;
  last_inference?: string;
  error?: string;
  is_loaded?: boolean;
  error_message?: string;
  model_name?: string;
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
  schema_version: string; // Required by backend API
  model_id: string;
  valid: boolean;
  issues: Array<{ type: string; message: string }>;
  download_commands?: string[];
  can_load?: boolean;
  reason?: string;
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

export interface AllModelsStatusResponse {
  schema_version: string; // Required by backend API
  models: ModelStatusResponse[];
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

// Tenant response types
export interface TenantResponse {
  schema_version: string; // Required by backend API
  id: string;
  name: string;
  uid: number;
  gid: number;
  isolation_metadata?: Record<string, unknown>;
  created_at: string;
  adapter_count?: number;
  user_count?: number;
}

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

export interface QuarantineStatusResponse {
  quarantined_count: number;
  quarantined_adapters: Array<{
    id: string;
    reason: string;
    created_at: string;
  }>;
  in_active_stacks: boolean;
}

export type NodeHealth = 'ok' | 'warning' | 'critical';

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
  candidates?: RouterCandidateInfo[]; // Extended: detailed candidate info instead of string[]
  k_value?: number; // Extended: K-sparse value
  adapter_map?: Map<number, string>; // Extended: For debugging: maps adapter_idx to adapter_id
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
}

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
