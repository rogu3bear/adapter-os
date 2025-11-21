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

export interface ErrorResponse {
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
  tokens_per_sec?: number;
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
}

export interface BaseModelStatus {
  status: string;
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
}

export interface ReplayVerificationResponse {
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
  ip_address: string;
  capabilities: string[];
  metadata?: Record<string, unknown>;
}

// Worker types
export interface SpawnWorkerRequest {
  worker_type: string;
  node_id?: string;
  config?: Record<string, unknown>;
}

export interface WorkerResponse {
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
  name: string;
  description?: string;
  steps: Omit<PlanStep, 'id' | 'status'>[];
}

export interface PlanComparisonResponse {
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
  k: number;
  threshold: number;
  strategy: 'top_k' | 'threshold' | 'hybrid';
  k_sparse?: number;
  entropy_floor?: number;
  gate_quant?: string;
  sample_tokens_full?: number;
}

// Promotion types
export interface PromotionRequest {
  adapter_id: string;
  from_tier: string;
  to_tier: string;
  reason?: string;
  gates?: PromotionGate[];
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
  name: string;
  url: string;
  branch?: string;
  credentials?: {
    type: 'ssh' | 'token';
    value: string;
  };
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
  seed?: number;
  require_evidence?: boolean;
  adapters?: string[];
}

export interface InferResponse {
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
  severity: 'info' | 'warning' | 'error' | 'critical';
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
  pack_id: string;
  name: string;
  version: string;
  policies: Policy[];
  enabled: boolean;
  created_at: string;
}

export interface SignPolicyResponse {
  policy_id: string;
  signature: string;
  signed_at: string;
  signed_by: string;
  cpid?: string;
}

export interface PolicyComparisonResponse {
  policy_a: Policy;
  policy_b: Policy;
  differences: Array<{
    path: string;
    type: 'added' | 'removed' | 'modified';
    value_a?: unknown;
    value_b?: unknown;
  }>;
  removed_keys?: string[];
  metallib_hash_changed?: boolean;
  added_keys?: string[];
}

export interface ExportPolicyResponse {
  policy_id: string;
  format: 'json' | 'yaml';
  content: string;
}

// Model import/validation types
export interface ImportModelRequest {
  source: 'huggingface' | 'local' | 'url';
  model_id: string;
  name?: string;
  quantization?: string;
}

export interface ImportModelResponse {
  import_id: string;
  model_id: string;
  status: 'pending' | 'downloading' | 'converting' | 'completed' | 'failed';
  progress?: number;
  error?: string;
}

export interface ModelValidationResponse {
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
  model_id: string;
  download_url: string;
  expires_at: string;
  size_bytes: number;
  artifacts?: ModelDownloadArtifact[];
}

export interface AllModelsStatusResponse {
  models: ModelStatusResponse[];
  total_memory_mb: number;
  available_memory_mb: number;
}

// Routing debug types
export interface RoutingDebugRequest {
  prompt: string;
  top_k?: number;
  include_scores?: boolean;
}

export interface RoutingDebugResponse {
  selected_adapters: string[];
  all_scores: Record<string, number>;
  gate_values: number[];
  decision_time_ms: number;
}

// Scan types
export interface ScanStatusResponse {
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
  tenant_id: string;
  assigned_policies: string[];
  updated_at: string;
}

export interface AssignAdaptersResponse {
  tenant_id: string;
  assigned_adapters: string[];
  updated_at: string;
}

// Promotion response types
export interface DryRunPromotionResponse {
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
  bundle_id: string;
  download_url: string;
  expires_at: string;
  format: 'json' | 'parquet';
  size_bytes?: number;
  events_count?: number;
}

export interface VerifyBundleSignatureResponse {
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
  deleted_count: number;
  freed_bytes: number;
  oldest_remaining?: string;
}

// Repository response types
export interface RepositoryReportResponse {
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
  by_type: Record<string, number>;
}

export interface UnifiedTelemetryEvent extends TelemetryEvent {
  source?: string;
  correlation_id?: string;
  id?: string;
  message?: string;
}

export interface Trace {
  trace_id: string;
  spans: Array<{
    span_id: string;
    name: string;
    start_time: string;
    end_time: string;
    attributes?: Record<string, unknown>;
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
  dismissed?: boolean;
  dismissed_at?: string;
  dismissible?: boolean;
  trigger?: string;
}

export interface UpdateDashboardConfigResponse {
  success: boolean;
  config_id: string;
  updated_at: string;
}

export interface ProcessLog {
  process_id: string;
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
  metadata?: Record<string, unknown>;
}

export interface ProcessCrash {
  process_id: string;
  timestamp: string;
  exit_code: number;
  signal?: string;
  stack_trace?: string;
}

export interface MetricsSnapshotResponse {
  timestamp: string;
  metrics: Record<string, number>;
  labels?: Record<string, string>;
  counters?: Record<string, number>;
  gauges?: Record<string, number>;
  histograms?: Record<string, unknown>;
}

export interface MetricsSeriesResponse {
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
  success: boolean;
  config_id: string;
  reset_at: string;
}

export interface ComplianceAuditResponse {
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
  step_id: string;
  title: string;
  description: string;
  action?: string;
  expected_result?: string;
}

export interface RoutingDecisionFilters {
  adapter_id?: string;
  start_time?: string;
  end_time?: string;
  min_score?: number;
  limit?: number;
  offset?: number;
}

export interface ResolveAlertRequest {
  alert_id: string;
  resolution?: string;
  resolved_by?: string;
}

export interface TransformedRoutingDecision extends RoutingDecision {
  transformed?: boolean;
  display_adapters?: string[];
}

export interface DebugSessionConfig {
  session_type: string;
  target_process?: string;
  breakpoints?: Array<{ file: string; line: number }>;
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
  severity?: string;
  status?: string;
  start_time?: string;
  end_time?: string;
  limit?: number;
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
  bundle_id: string;
  name?: string;
  config?: Record<string, unknown>;
}

// Inference session for tracking inference history
export interface InferenceSession {
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
