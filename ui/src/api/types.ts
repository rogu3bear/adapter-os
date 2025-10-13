// API Types matching mplora-server-api

// Authentication
export interface LoginRequest {
  email: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user_id: string;
  role: string;
}

export interface UserInfoResponse {
  user_id: string;
  email: string;
  role: string;
}

export interface User {
  id: string;
  email: string;
  display_name: string;
  role: UserRole;
  tenant_id: string;
  permissions: string[];
}

export type UserRole = 'Admin' | 'Operator' | 'SRE' | 'Compliance' | 'Auditor' | 'Viewer';

// Tenants
export interface Tenant {
  id: string;
  name: string;
  created_at: string;
  isolation_level: string;
  // Extended fields for UI and future backend expansion
  status?: 'active' | 'paused' | 'archived' | 'maintenance' | 'suspended';
  description?: string;
  data_classification?: 'internal' | 'restricted' | 'confidential' | 'public';
  users?: number;
  adapters?: number;
  policies?: number;
  itar_compliant?: boolean;
  last_activity?: string;
}

export interface CreateTenantRequest {
  name: string;
  isolation_level?: string;
}

// Nodes
export interface Node {
  id: string;
  tenant_id: string;
  hostname: string;
  metal_family: string;
  memory_gb: number;
  status: string;
  last_heartbeat: string;
}

export interface RegisterNodeRequest {
  hostname: string;
  metal_family: string;
  memory_gb: number;
}

export interface NodePingResponse {
  node_id: string;
  status: string;
  latency_ms: number;
}

export interface WorkerInfo {
  id: string;
  tenant_id: string;
  plan_id: string;
  status: string;
}

export interface NodeDetailsResponse {
  id: string;
  hostname: string;
  agent_endpoint: string;
  status: string;
  last_seen_at: string | null;
  workers: WorkerInfo[];
  recent_logs: string[];
  // Extended fields for resource monitoring
  metal_family?: string;
  memory_gb?: number;
  gpu_count?: number;
  gpu_type?: string;
  last_heartbeat?: string;
}

// Plans
export interface Plan {
  id: string;
  cpid: string;
  status: string;
  created_at: string;
  metallib_hash?: string;
}

export interface BuildPlanRequest {
  model_name: string;
  adapters: string[];
}

export interface PlanComparisonResponse {
  plan_1: string;
  plan_2: string;
  differences: string[];
  metallib_hash_changed: boolean;
  adapter_changes: {
    added: string[];
    removed: string[];
  };
}

// Control Plane
export interface PromotionRequest {
  cpid: string;
  skip_gates?: boolean;
}

export interface PromotionGate {
  name: string;
  status: 'passed' | 'failed' | 'pending';
  message: string;
}

export interface PromotionRecord {
  id: string;
  cpid: string;
  promoted_at: string;
  promoted_by: string;
  gates_passed: boolean;
}

// Policies
export interface Policy {
  cpid: string;
  policy_json: string;
  schema_hash: string;
}

export interface ValidatePolicyRequest {
  policy_json: string;
}

export interface ApplyPolicyRequest {
  cpid: string;
  policy_json: string;
}

// Telemetry
export interface TelemetryBundle {
  id: string;
  cpid: string;
  event_count: number;
  size_bytes: number;
  merkle_root: string;
  created_at: string;
}

// Adapters
export interface Adapter {
  id: string;
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  tier: number;
  languages_json?: string;
  framework?: string;
  
  // Code intelligence fields
  category: AdapterCategory;
  scope: AdapterScope;
  framework_id?: string;
  framework_version?: string;
  repo_id?: string;
  commit_sha?: string;
  intent?: string;
  
  // Lifecycle state management
  current_state: AdapterState;
  pinned: boolean;
  memory_bytes: number;
  last_activated?: string;
  activation_count: number;
  
  created_at: string;
  updated_at: string;
  active: boolean;
}

export type AdapterCategory = 'code' | 'framework' | 'codebase' | 'ephemeral';
export type AdapterScope = 'global' | 'tenant' | 'repo' | 'commit';
export type AdapterState = 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
export type EvictionPriority = 'never' | 'low' | 'normal' | 'high' | 'critical';

export interface RegisterAdapterRequest {
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  tier: number;
  languages_json?: string;
  framework?: string;
  
  // Code intelligence fields
  category: AdapterCategory;
  scope: AdapterScope;
  framework_id?: string;
  framework_version?: string;
  repo_id?: string;
  commit_sha?: string;
  intent?: string;
}

export interface AdapterActivation {
  adapter_id: string;
  activation_pct: number;
  token_count: number;
}

export interface AdapterStats {
  total_activations: number;
  avg_activation_pct: number;
  quality_delta: number;
}

export interface AdapterStateResponse {
  adapter_id: string;
  old_state: string;
  new_state: string;
  timestamp: string;
}

export interface AdapterManifest {
  adapter_id: string;
  name: string;
  hash_b3: string;
  rank: number;
  tier: number;
  framework?: string;
  languages_json?: string;
  category?: string;
  scope?: string;
  framework_id?: string;
  framework_version?: string;
  repo_id?: string;
  commit_sha?: string;
  intent?: string;
  created_at: string;
  updated_at: string;
}

export interface AdapterHealthResponse {
  adapter_id: string;
  total_activations: number;
  selected_count: number;
  avg_gate_value: number;
  memory_usage_mb: number;
  policy_violations: string[];
  recent_activations: AdapterActivation[];
}

// Adapter Lifecycle Management
export interface AdapterStateRecord {
  adapter_id: string;
  adapter_idx: number;
  state: AdapterState;
  pinned: boolean;
  memory_bytes: number;
  category: AdapterCategory;
  scope: AdapterScope;
  last_activated?: string;
  activation_count: number;
}

export interface CategoryPolicy {
  promotion_threshold_ms: number;
  demotion_threshold_ms: number;
  memory_limit: number;
  eviction_priority: EvictionPriority;
  auto_promote: boolean;
  auto_demote: boolean;
  max_in_memory?: number;
  routing_priority: number;
}

export interface AdapterStateSummary {
  category: AdapterCategory;
  scope: AdapterScope;
  state: AdapterState;
  count: number;
  total_memory_bytes: number;
  avg_activations: number;
  most_recent_activation?: string;
}

export interface MemoryUsageByCategory {
  [category: string]: number;
}

export interface AdapterTransitionEvent {
  adapter_id: string;
  from_state: AdapterState;
  to_state: AdapterState;
  reason: string;
  timestamp: string;
}

export interface AdapterActivationEvent {
  adapter_id: string;
  state: AdapterState;
  category: AdapterCategory;
  activation_count: number;
  timestamp: string;
}

export interface AdapterEvictionEvent {
  adapter_id: string;
  from_state: AdapterState;
  category: AdapterCategory;
  memory_freed: number;
  timestamp: string;
}

// Repositories
export interface Repository {
  id: string;
  url: string;
  branch: string;
  last_scan?: string;
  commit_count: number;
}

export interface RegisterRepositoryRequest {
  url: string;
  branch: string;
}

export interface ScanStatusResponse {
  status: string;
  commits_processed: number;
  last_commit?: string;
}

// Commits
export interface Commit {
  sha: string;
  message: string;
  author: string;
  timestamp: string;
  diff_stats?: DiffStats;
}

export interface DiffStats {
  files_changed: number;
  insertions: number;
  deletions: number;
}

export interface CommitDiff {
  sha: string;
  diff: string;
  stats: DiffStats;
}

// Metrics
export interface SystemMetrics {
  memory_usage_pct: number;
  adapter_count: number;
  active_sessions: number;
  tokens_per_second: number;
  latency_p95_ms: number;
  // Extended fields for detailed resource monitoring
  cpu_usage_percent?: number;
  cpu_cores?: number;
  cpu_temp_celsius?: number;
  memory_used_gb?: number;
  memory_total_gb?: number;
  memory_usage_percent?: number;
  gpu_utilization_percent?: number;
  gpu_memory_used_gb?: number;
  gpu_memory_total_gb?: number;
  gpu_temp_celsius?: number;
  gpu_power_watts?: number;
  disk_used_gb?: number;
  disk_total_gb?: number;
  disk_usage_percent?: number;
  disk_read_mbps?: number;
  disk_write_mbps?: number;
  network_rx_bytes?: number;
  network_tx_bytes?: number;
  network_rx_packets?: number;
  network_tx_packets?: number;
  current_loss?: number;
  learning_rate?: number;
  current_epoch?: number;
  total_epochs?: number;
}

export interface QualityMetrics {
  arr: number;
  ecs5: number;
  hlr: number;
  cr: number;
}

export interface AdapterMetrics {
  adapter_id: string;
  performance: AdapterPerformance;
}

export interface AdapterPerformance {
  avg_latency_ms: number;
  quality_score: number;
  activation_count: number;
}

// Routing
export interface RoutingDebugRequest {
  prompt: string;
  adapters?: string[];
}

export interface RoutingDebugResponse {
  selected_adapters: AdapterScore[];
  feature_vector: FeatureVector;
}

export interface AdapterScore {
  adapter_id: string;
  score: number;
  gate_value: number;
}

export interface FeatureVector {
  prompt_embedding: number[];
  context_tokens: number;
}

export interface RoutingDecision {
  timestamp: string;
  prompt_hash: string;
  adapters: string[];
  gates: number[];
}

// Inference
export interface InferRequest {
  prompt: string;
  max_tokens?: number;
  temperature?: number;
}

export interface InferResponse {
  text: string;
  trace: InferenceTrace;
}

export interface InferenceTrace {
  router_decisions: RouterDecision[];
  evidence_spans: EvidenceSpan[];
  latency_ms: number;
}

export interface RouterDecision {
  token_idx: number;
  adapters: string[];
  gates: number[];
}

export interface EvidenceSpan {
  doc_id: string;
  span_hash: string;
  text: string;
}

// Error Response
export interface ErrorResponse {
  error: string;
  details?: string;
}

// Health
export interface HealthResponse {
  status: string;
  version?: string;
}

// Training
export interface TrainingJob {
  id: string;
  adapter_name: string;
  template_id?: string;
  repo_id?: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress_pct: number;
  current_epoch: number;
  total_epochs: number;
  current_loss: number;
  learning_rate: number;
  tokens_per_second: number;
  created_at: string;
  started_at?: string;
  completed_at?: string;
  error_message?: string;
}

export interface TrainingConfig {
  rank: number;
  alpha: number;
  targets: string[];
  epochs: number;
  learning_rate: number;
  batch_size: number;
  warmup_steps?: number;
  max_seq_length?: number;
  gradient_accumulation_steps?: number;
}

export interface StartTrainingRequest {
  adapter_name: string;
  config: TrainingConfig;
  template_id?: string;
  repo_id?: string;
}

export interface TrainingMetrics {
  loss: number;
  tokens_per_second: number;
  learning_rate: number;
  current_epoch: number;
  total_epochs: number;
  progress_pct: number;
}

export interface TrainingTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
  rank: number;
  alpha: number;
  targets: string[];
  epochs: number;
  learning_rate: number;
  batch_size: number;
}

// Meta
export interface MetaResponse {
  version: string;
  cpid?: string;
  build_info?: Record<string, string>;
}

// ===== Phase 6: Policy Operations =====
export interface SignPolicyResponse {
  cpid: string;
  signature: string;
  signed_at: string;
  signed_by: string;
}

export interface PolicyComparisonRequest {
  cpid_1: string;
  cpid_2: string;
}

export interface PolicyComparisonResponse {
  cpid_1: string;
  cpid_2: string;
  differences: string[];
  added_keys: string[];
  removed_keys: string[];
  schema_version_changed: boolean;
}

export interface ExportPolicyResponse {
  cpid: string;
  policy_json: string;
  signature: string;
  hash_b3: string;
  created_at: string;
}

// ===== Phase 7: Promotion Execution =====
export interface DryRunPromotionRequest {
  cpid: string;
}

export interface DryRunPromotionResponse {
  cpid: string;
  would_promote: boolean;
  gate_results: [string, boolean, string | null][];
  validation_errors: string[];
  simulated_at: string;
}

export interface PromotionHistoryEntry {
  cpid: string;
  promoted_at: string;
  promoted_by: string;
  previous_cpid: string | null;
  gate_results_summary: string;
}

// ===== Phase 8: Telemetry Operations =====
export interface ExportTelemetryBundleResponse {
  bundle_id: string;
  events_count: number;
  size_bytes: number;
  download_url: string;
  expires_at: string;
}

export interface VerifyBundleSignatureResponse {
  bundle_id: string;
  valid: boolean;
  signature: string;
  signed_by: string;
  signed_at: string;
  verification_error: string | null;
}

export interface PurgeOldBundlesRequest {
  keep_bundles_per_cpid: number;
}

export interface PurgeOldBundlesResponse {
  purged_count: number;
  retained_count: number;
  freed_bytes: number;
  purged_cpids: string[];
}

// ===== Phase 9: Code Intelligence =====
export interface RepositoryReportResponse {
  repo_id: string;
  total_lines: number;
  complexity_score: number;
  risk_level: string;
  languages: [string, number][];
  last_analyzed: string;
}

// ===== Phase 10: Tenant Management =====
export interface UpdateTenantRequest {
  name: string;
}

export interface TenantResponse {
  id: string;
  name: string;
  itar_flag: boolean;
  created_at: string;
}

export interface AssignPoliciesRequest {
  cpids: string[];
}

export interface AssignPoliciesResponse {
  tenant_id: string;
  assigned_cpids: string[];
  assigned_at: string;
}

export interface AssignAdaptersRequest {
  adapter_ids: string[];
}

export interface AssignAdaptersResponse {
  tenant_id: string;
  assigned_adapter_ids: string[];
  assigned_at: string;
}

export interface TenantUsageResponse {
  tenant_id: string;
  cpu_usage_pct: number;
  gpu_usage_pct: number;
  memory_used_gb: number;
  memory_total_gb: number;
  inference_count_24h: number;
  active_adapters_count: number;
  // Optional legacy fields
  active_sessions?: number;
  avg_latency_ms?: number;
  estimated_cost_usd?: number;
}

// Domain Adapter Types
export interface DomainAdapter {
  id: string;
  name: string;
  version: string;
  description: string;
  domain_type: 'text' | 'vision' | 'telemetry';
  model: string;
  hash: string;
  input_format: string;
  output_format: string;
  config: Record<string, any>;
  status: 'loaded' | 'unloaded' | 'error';
  epsilon_stats?: EpsilonStats;
  last_execution?: string;
  execution_count: number;
  created_at: string;
  updated_at: string;
}

export interface EpsilonStats {
  mean_error: number;
  max_error: number;
  error_count: number;
  last_updated: string;
}

export interface CreateDomainAdapterRequest {
  name: string;
  version: string;
  description: string;
  domain_type: 'text' | 'vision' | 'telemetry';
  model: string;
  hash: string;
  input_format: string;
  output_format: string;
  config: Record<string, any>;
}

export interface TestDomainAdapterRequest {
  adapter_id: string;
  input_data: string;
  expected_output?: string;
  iterations?: number;
}

export interface TestDomainAdapterResponse {
  test_id: string;
  adapter_id: string;
  input_data: string;
  actual_output: string;
  expected_output?: string;
  epsilon?: number;
  passed: boolean;
  iterations: number;
  execution_time_ms: number;
  executed_at: string;
}

export interface DomainAdapterManifest {
  adapter_id: string;
  name: string;
  version: string;
  description: string;
  domain_type: string;
  model: string;
  hash: string;
  input_format: string;
  output_format: string;
  config: Record<string, any>;
  created_at: string;
  updated_at: string;
}

export interface DomainAdapterExecutionResponse {
  execution_id: string;
  adapter_id: string;
  input_hash: string;
  output_hash: string;
  epsilon: number;
  execution_time_ms: number;
  trace_events: string[];
  executed_at: string;
}
