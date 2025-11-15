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
  email?: string;
  display_name?: string;
  tenant_id?: string;
  permissions?: string[];
  last_login_at?: string;
  token_last_rotated_at?: string;
}

export interface UserInfoResponse {
  user_id: string;
  email: string;
  role: string;
  display_name?: string;
  tenant_id?: string;
  permissions?: string[];
  last_login_at?: string;
  mfa_enabled?: boolean;
  token_last_rotated_at?: string;
}

export interface User {
  id: string;
  email: string;
  display_name: string;
  role: UserRole;
  tenant_id: string;
  permissions: string[];
  roles?: string[];
  last_login_at?: string;
  mfa_enabled?: boolean;
  token_last_rotated_at?: string;
}

export type UserRole = 'admin' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';

export interface SessionInfo {
  id: string;
  device?: string;
  ip_address?: string;
  user_agent?: string;
  location?: string;
  created_at: string;
  last_seen_at: string;
  is_current: boolean;
}

export interface RotateTokenResponse {
  token: string;
  created_at: string;
  expires_at?: string;
  last_rotated_at?: string;
}

export interface TokenMetadata {
  created_at: string;
  expires_at?: string;
  last_rotated_at?: string;
  last_used_at?: string;
}

export interface AuthConfigResponse {
  production_mode: boolean;
  dev_token_enabled: boolean;
  jwt_mode: string;
  token_expiry_hours: number;
}

export interface UpdateAuthConfigRequest {
  production_mode?: boolean;
  dev_token_enabled?: boolean;
  jwt_mode?: string;
  token_expiry_hours?: number;
}

export interface UpdateProfileRequest {
  display_name?: string;
  avatar_url?: string;
}

export interface ProfileResponse extends UserInfoResponse {}

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
  itarCompliant?: boolean;
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
  agent_endpoint?: string;
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
  tenant_id: string;
  manifest_hash_b3: string;
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
  manifest_hash_b3?: string;
  policy_hash_b3?: string;
}

// Unified telemetry event for logs API
export interface UnifiedTelemetryEvent {
  id: string;
  timestamp: string;
  event_type: string;
  level: 'Debug' | 'Info' | 'Warn' | 'Error' | 'Critical';
  message: string;
  component?: string;
  tenant_id?: string;
  trace_id?: string;
  metadata?: Record<string, string | number | boolean>;
}

// Metrics types
export interface MetricsSnapshotResponse {
  timestamp: number;
  counters: Record<string, number>;
  gauges: Record<string, number>;
  histograms: Record<string, any>;
}

export interface MetricsSeriesResponse {
  series_name: string;
  points: MetricDataPointResponse[];
}

export interface MetricDataPointResponse {
  timestamp: number;
  value: number;
  labels?: Record<string, string>;
}

// Trace types
export interface Trace {
  trace_id: string;
  spans: Span[];
  root_span_id: string | null;
}

export interface Span {
  span_id: string;
  trace_id: string;
  parent_id: string | null;
  name: string;
  start_ns: number;
  end_ns: number | null;
  attributes: Record<string, any>;
  status: 'ok' | 'error' | 'unset';
}

// Golden baselines
export interface GoldenRunSummary {
  name: string;
  run_id: string;
  cpid: string;
  plan_id: string;
  bundle_hash: string;
  layer_count: number;
  max_epsilon: number;
  mean_epsilon: number;
  toolchain_summary: string;
  adapters: string[];
  created_at: string;
  has_signature: boolean;
}

export interface GoldenRun extends GoldenRunSummary {
  metrics?: Array<{ key: string; value: string | number }>;
}

export interface GoldenCompareMetric {
  key: string;
  value1: string | number;
  value2: string | number;
  diff: string | number;
}

export interface GoldenCompareResult {
  run_id_1: string;
  run_id_2: string;
  metrics: GoldenCompareMetric[];
  notes?: string[];
}

export type Strictness = 'bitwise' | 'epsilon-tolerant' | 'statistical';

export interface GoldenCompareRequest {
  golden: string;
  bundle_id: string;
  strictness?: Strictness;
  epsilon_tolerance?: number; // Optional epsilon threshold for epsilon-tolerant strictness
  verify_toolchain?: boolean;
  verify_adapters?: boolean;
  verify_device?: boolean;
  verify_signature?: boolean;
}

// Epsilon comparison structures
export interface EpsilonStats {
  l2_error: number;
  max_error: number;
  mean_error: number;
  element_count: number;
}

export interface LayerDivergence {
  layer_id: string;
  golden: EpsilonStats;
  current: EpsilonStats;
  relative_error: number;
}

export interface EpsilonComparison {
  matching_layers: string[];
  divergent_layers: LayerDivergence[];
  missing_in_current: string[];
  missing_in_golden: string[];
  tolerance: number;
  pass_rate?: number; // Percentage of layers passing tolerance (0-100)
}

export interface ToolchainMetadata {
  rustc_version: string;
  metal_version: string;
  kernel_hash: string;
}

export interface DeviceFingerprint {
  schema_version: number;
  device_model: string;
  soc_id: string;
  gpu_pci_id: string;
  os_version: string;
  os_build: string;
  metal_family: string;
  gpu_driver_version: string;
  path_hash: string;
  env_hash: string;
  cpu_features: string[];
  firmware_hash?: string | null;
  boot_version_hash?: string | null;
}

export interface GoldenRunMetadata {
  run_id: string;
  cpid: string;
  plan_id: string;
  created_at: string;
  toolchain: ToolchainMetadata;
  adapters: string[];
  device: DeviceFingerprint;
  global_seed: string;
}

export interface VerificationReport {
  passed: boolean;
  golden_metadata: GoldenRunMetadata;
  current_metadata: GoldenRunMetadata;
  bundle_hash_match: boolean;
  signature_verified: boolean;
  epsilon_comparison: EpsilonComparison;
  toolchain_compatible: boolean;
  adapters_compatible: boolean;
  device_compatible: boolean;
  messages: string[];
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

// Adapter Policy Management
export interface UpdateAdapterPolicyRequest {
  category?: AdapterCategory;
}

export interface UpdateAdapterPolicyResponse {
  adapter_id: string;
  category?: AdapterCategory;
  message: string;
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
  url_is_fallback?: boolean;
  branch: string;
  path?: string;
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

// Git Repository API Types
export interface RegisterGitRepositoryRequest {
  repo_id: string;
  path: string;
  branch?: string;
  description?: string;
}

export interface GitInfo {
  branch: string;
  commit_count: number;
  last_commit: string; // Commit message/summary, not timestamp
  authors: string[];
}

export interface LanguageInfo {
  name: string;
  files: number;
  lines: number;
  percentage: number;
}

export interface FrameworkInfo {
  name: string;
  version?: string;
  confidence: number;
  files: string[];
}

export interface SecurityViolation {
  file_path: string;
  pattern: string;
  line_number?: number;
  severity: string;
}

export interface SecurityScanResult {
  violations: SecurityViolation[];
  scan_timestamp: string;
  status: string;
}

export interface EvidenceSpan {
  span_id: string;
  evidence_type: string;
  file_path: string;
  line_range: [number, number];
  relevance_score: number;
  content: string;
}

export interface RepositoryAnalysis {
  repo_id: string;
  languages: LanguageInfo[];
  frameworks: FrameworkInfo[];
  security_scan: SecurityScanResult;
  git_info: GitInfo;
  evidence_spans: EvidenceSpan[];
}

export interface RegisterGitRepositoryResponse {
  repo_id: string;
  status: string;
  analysis: RepositoryAnalysis;
  evidence_count: number;
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
// Citation: API contract for system metrics from adapteros-server-api
export interface SystemMetrics {
  // Core metrics (always present)
  memory_usage_pct: number;
  adapter_count: number;
  active_sessions: number;
  tokens_per_second: number;
  latency_p95_ms: number;

  // Extended fields for detailed resource monitoring (optional)
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

  // Training-specific fields (optional)
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
  avg_latency_ms?: number;
  avg_latency_us?: number;
  quality_score?: number;
  activation_count?: number;
  activation_rate?: number;
  total_requests?: number;
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
  id: string;
  timestamp: string;
  prompt_hash: string;
  input_hash?: string;
  adapters: string[];
  gates: number[];
  total_score?: number;
  k_value?: number;
  entropy?: number;
  adapter_selections?: AdapterSelection[];
  confidence_scores?: Record<string, number>;
  trace_id?: string;
}

// Inference
export interface InferRequest {
  prompt: string;
  max_tokens?: number;
  temperature?: number;
  top_k?: number;
  top_p?: number;
  seed?: number;
  require_evidence?: boolean;
  adapters?: string[];
}

export interface InferResponse {
  text: string;
  token_count?: number;
  finish_reason?: 'stop' | 'length' | 'error' | string;
  latency_ms?: number;
  trace: InferenceTrace | DetailedInferenceTrace;
}

export interface InferenceTrace {
  router_decisions: RouterDecision[];
  evidence_spans: EvidenceSpan[];
  latency_ms: number;
}

// Batch Inference Types
export interface BatchInferItemRequest {
  id: string;
  prompt: string;
  max_tokens?: number;
  temperature?: number;
  top_k?: number;
  top_p?: number;
  seed?: number;
  require_evidence?: boolean;
  adapters?: string[];
}

export interface BatchInferRequest {
  requests: BatchInferItemRequest[];
}

export interface BatchInferItemResponse {
  id: string;
  response?: InferResponse;
  error?: ErrorResponse;
}

export interface BatchInferResponse {
  responses: BatchInferItemResponse[];
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
  message?: string;
  code?: string;
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
  progress?: number;
  progress_pct?: number;
  current_epoch?: number;
  total_epochs?: number;
  current_loss?: number;
  learning_rate?: number;
  tokens_per_second?: number;
  created_at: string;
  started_at?: string;
  completed_at?: string;
  error_message?: string;
  // Orchestrator-populated fields when packaging is enabled
  artifact_path?: string;
  adapter_id?: string;
  weights_hash_b3?: string;
  config?: TrainingConfig;
  metrics?: TrainingMetrics;
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
  category?: AdapterCategory;
  scope?: AdapterScope;
  repo_id?: string;
  commit_sha?: string;
  framework_id?: string;
  framework_version?: string;
  repo_scope?: string;
  dataset_path?: string;
}

export interface StartTrainingRequest {
  adapter_name: string;
  config: TrainingConfig;
  template_id?: string;
  repo_id?: string;
  dataset_path?: string;
  adapters_root?: string;
  package?: boolean;
  register?: boolean;
  adapter_id?: string;
  tier?: number;
  // New optional fields aligned with server
  // Absolute directory root required by dataset builder
  directory_root?: string;
  // Directory path relative to root (defaults to ".")
  directory_path?: string;
  // Optional tenant context
  tenant_id?: string;
}

export interface TrainingMetrics {
  loss: number;
  tokens_per_second: number;
  learning_rate: number;
  current_epoch: number;
  total_epochs: number;
  progress_pct: number;
  validation_loss?: number;
  gpu_utilization?: number;
  memory_usage?: number;
}

export interface TrainingArtifactsResponse {
  artifact_path?: string;
  adapter_id?: string;
  weights_hash_b3?: string;
  manifest_hash_b3?: string;
  manifest_hash_matches: boolean;
  signature_valid: boolean;
  ready: boolean;
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

export interface TrainingSession {
  session_id: string;
  status: 'pending' | 'running' | 'paused' | 'completed' | 'failed';
  progress: number;
  adapter_name: string;
  repository_path: string;
  created_at: string;
  updated_at: string;
  error_message?: string;
}

export interface PauseTrainingResponse {
  session_id: string;
  status: 'paused';
  message: string;
}

export interface ResumeTrainingResponse {
  session_id: string;
  status: 'running';
  message: string;
}

// Meta
export interface MetaResponse {
  version: string;
  cpid?: string;
  build_info?: Record<string, string>;
  build_hash?: string;
  uptime?: number;
  last_updated?: string;
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
  total_files: number;
  total_lines: number;
  complexity_score: number;
  risk_level: string;
  languages: Record<string, { files: number; lines: number }>;
  ephemeral_adapters_count?: number;
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
//! Strongly typed configuration for domain adapters
//! 
//! # Citations
//! - TypeScript best practices: Avoid `any` types for type safety
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"

export interface DomainAdapterConfig {
  temperature?: number;
  max_tokens?: number;
  top_p?: number;
  frequency_penalty?: number;
  presence_penalty?: number;
  stop_sequences?: string[];
  custom_parameters?: Record<string, string | number | boolean>;
}

//! Strongly typed notification and alerting configuration
export interface NotificationChannel {
  type: 'email' | 'webhook' | 'slack' | 'pagerduty';
  endpoint?: string;
  recipients?: string[];
  enabled: boolean;
  settings?: Record<string, string | number | boolean>;
}

export interface EscalationRule {
  level: number;
  delay_minutes: number;
  notification_channels: string[];
  conditions?: Record<string, string | number | boolean>;
}

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
  config: DomainAdapterConfig; // Strongly typed instead of any
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

// Base Model Status Types
export interface BaseModelStatus {
  model_id: string;
  model_name: string;
  status: 'loading' | 'loaded' | 'unloading' | 'unloaded' | 'error';
  loaded_at?: string;
  unloaded_at?: string;
  error_message?: string;
  memory_usage_mb?: number;
  is_loaded: boolean;
  updated_at: string;
}

// Base Model Import Types - Citation: IMPLEMENTATION_PLAN.md Phase 2
export interface ImportModelRequest {
  model_name: string;
  weights_path: string;
  config_path: string;
  tokenizer_path: string;
  tokenizer_config_path?: string;
  metadata?: Record<string, any>;
}

export interface ImportModelResponse {
  import_id: string;
  status: 'uploading' | 'validating' | 'importing' | 'completed' | 'failed';
  message: string;
  progress?: number;
}

export interface ModelStatusResponse {
  model_id: string;
  model_name: string;
  status: 'loading' | 'loaded' | 'unloading' | 'unloaded' | 'error';
  loaded_at?: string;
  memory_usage_mb?: number;
  is_loaded: boolean;
}

export interface ModelDownloadArtifact {
  artifact: string;
  filename: string;
  content_type: string;
  size_bytes?: number;
  download_url: string;
  expires_at: string;
}

export interface ModelDownloadResponse {
  model_id: string;
  model_name: string;
  artifacts: ModelDownloadArtifact[];
}

// Multi-model status response
export interface AllModelsStatusResponse {
  models: BaseModelStatus[];
  total_memory_mb: number;
  active_model_count: number;
}

// Model validation response for checking if a model can be loaded
export interface ModelValidationResponse {
  model_id: string;
  model_name: string;
  can_load: boolean;
  reason?: string;
  download_commands?: string[];
}

// OpenAI-compatible models list (used by ModelSelector)
export interface OpenAIModelInfo {
  id: string;
  object: string; // usually 'model'
  created: number;
  owned_by: string;
}

export interface OpenAIModelsListResponse {
  object: string; // 'list'
  data: OpenAIModelInfo[];
}

export interface CursorConfigResponse {
  api_endpoint: string;
  model_name: string;
  model_id: string;
  is_ready: boolean;
  setup_instructions: string[];
}

export interface OnboardingJourneyStep {
  step_completed: 'model_imported' | 'model_loaded' | 'cursor_configured' | 'first_inference';
  completed_at: string;
  step_data?: Record<string, any>;
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
  config: DomainAdapterConfig; // Strongly typed instead of any
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
  config: DomainAdapterConfig; // Strongly typed instead of any
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

// Replay Session Types
export interface ReplaySession {
  id: string;
  tenant_id: string;
  cpid: string;
  plan_id: string;
  snapshot_at: string;
  seed_global_b3: string;
  manifest_hash_b3: string;
  policy_hash_b3: string;
  kernel_hash_b3?: string;
  telemetry_bundle_ids: string[];
  adapter_state: AdapterStateSnapshot;
  routing_decisions: RoutingDecision[];
  inference_traces?: InferenceTrace[];
  signature: string;
  created_at: string;
}

export interface AdapterStateSnapshot {
  adapters: Adapter[];
  timestamp: string;
  memory_usage_bytes: number;
}

export interface CreateReplaySessionRequest {
  tenant_id: string;
  cpid: string;
  plan_id: string;
  telemetry_bundle_ids: string[];
  snapshot_at?: string;
}

export interface ReplayVerificationResponse {
  session_id: string;
  signature_valid: boolean;
  hash_chain_valid: boolean;
  manifest_verified: boolean;
  policy_verified: boolean;
  kernel_verified: boolean;
  divergences: ReplayDivergence[];
  verified_at: string;
}

export interface ReplayDivergence {
  divergence_type: 'router' | 'adapter' | 'inference' | 'policy';
  expected_hash: string;
  actual_hash: string;
  context: string;
}

// Training Wizard Types
export interface TrainingWizardState {
  step: number;
  category: AdapterCategory;
  basicInfo: {
    name: string;
    description: string;
    scope: AdapterScope;
  };
  dataSource: {
    type: 'repository' | 'template' | 'custom';
    repositoryId?: string;
    templateId?: string;
    customData?: string;
  };
  categoryConfig: {
    // Code adapter
    language?: string;
    symbolTargets?: string[];
    // Framework adapter
    frameworkId?: string;
    frameworkVersion?: string;
    apiPatterns?: string[];
    // Codebase adapter
    repoScope?: string;
    filePatterns?: string[];
    // Ephemeral adapter
    ttlSeconds?: number;
    contextWindow?: number;
  };
  trainingParams: TrainingConfig;
}

// Policy Pack Configurations
export interface PolicyPackConfig {
  egress?: EgressConfig;
  determinism?: DeterminismConfig;
  router?: RouterConfig;
  evidence?: EvidenceConfig;
  refusal?: RefusalConfig;
  numeric?: NumericConfig;
  rag?: RagConfig;
  isolation?: IsolationConfig;
  telemetry?: TelemetryConfig;
  retention?: RetentionConfig;
  performance?: PerformanceConfig;
  memory?: MemoryConfig;
  artifacts?: ArtifactsConfig;
  secrets?: SecretsConfig;
  build_release?: BuildReleaseConfig;
  compliance?: ComplianceConfig;
  incident?: IncidentConfig;
  output?: OutputConfig;
  adapters?: AdaptersConfig;
}

export interface EgressConfig {
  mode: 'deny_all' | 'allow_list';
  serve_requires_pf: boolean;
  allow_tcp: boolean;
  allow_udp: boolean;
  uds_paths: string[];
  media_import?: {
    require_signature: boolean;
    require_sbom: boolean;
  };
}

export interface DeterminismConfig {
  require_metallib_embed: boolean;
  require_kernel_hash_match: boolean;
  rng: 'hkdf_seeded' | 'deterministic';
  retrieval_tie_break: string[];
}

export interface RouterConfig {
  k_sparse: number;
  gate_quant: 'q15' | 'q8' | 'f16';
  entropy_floor: number;
  sample_tokens_full: number;
}

export interface EvidenceConfig {
  require_open_book: boolean;
  min_spans: number;
  prefer_latest_revision: boolean;
  warn_on_superseded: boolean;
}

export interface RefusalConfig {
  abstain_threshold: number;
  missing_fields_templates: Record<string, string[]>;
}

export interface NumericConfig {
  canonical_units: Record<string, string>;
  max_rounding_error: number;
  require_units_in_trace: boolean;
}

export interface RagConfig {
  index_scope: 'per_tenant' | 'shared';
  doc_tags_required: string[];
  embedding_model_hash: string;
  topk: number;
  order: string[];
}

export interface IsolationConfig {
  process_model: 'per_tenant' | 'shared';
  uds_root: string;
  forbid_shm: boolean;
  keys: {
    backend: 'secure_enclave' | 'file';
    require_hardware: boolean;
  };
}

export interface TelemetryConfig {
  schema_hash: string;
  sampling: {
    token: number;
    router: number;
    inference: number;
  };
  router_full_tokens: number;
  bundle: {
    max_events: number;
    max_bytes: number;
  };
}

export interface RetentionConfig {
  keep_bundles_per_cpid: number;
  keep_incident_bundles: boolean;
  keep_promotion_bundles: boolean;
  evict_strategy: string;
}

export interface PerformanceConfig {
  latency_p95_ms: number;
  router_overhead_pct_max: number;
  throughput_tokens_per_s_min: number;
}

export interface MemoryConfig {
  min_headroom_pct: number;
  evict_order: string[];
  k_reduce_before_evict: boolean;
}

export interface ArtifactsConfig {
  require_signature: boolean;
  require_sbom: boolean;
  cas_only: boolean;
}

export interface SecretsConfig {
  env_allowed: string[];
  keystore: 'secure_enclave' | 'file';
  rotate_on_promotion: boolean;
}

export interface BuildReleaseConfig {
  require_replay_zero_diff: boolean;
  hallucination_thresholds: {
    arr_min: number;
    ecs5_min: number;
    hlr_max: number;
    cr_max: number;
  };
  require_signed_plan: boolean;
  require_rollback_plan: boolean;
}

export interface ComplianceConfig {
  control_matrix_hash: string;
  require_evidence_links: boolean;
  require_itar_suite_green: boolean;
}

export interface IncidentConfig {
  memory: string[];
  router_skew: string[];
  determinism: string[];
  violation: string[];
}

export interface OutputConfig {
  format: 'json' | 'text';
  require_trace: boolean;
  forbidden_topics: string[];
}

export interface AdaptersConfig {
  min_activation_pct: number;
  min_quality_delta: number;
  require_registry_admit: boolean;
}

// Worker Management Types
export interface SpawnWorkerRequest {
  node_id: string;
  tenant_id: string;
  plan_id: string;
}

export interface WorkerResponse {
  id: string;
  tenant_id: string;
  node_id: string;
  plan_id: string;
  uds_path: string;
  pid: number | null;
  status: string;
  started_at: string;
  last_seen_at: string | null;
}

export interface WorkerDetailsResponse {
  id: string;
  tenant_id: string;
  node_id: string;
  plan_id: string;
  uds_path: string;
  pid: number | null;
  status: string;
  started_at: string;
  last_seen_at: string | null;
  resource_usage?: {
    memory_mb: number;
    cpu_percent: number;
  };
  recent_activity?: string[];
}

// Inference Session Types (using EnhancedInferRequest types defined later)

export interface PolicyPackResponse {
  cpid: string;
  content: string;
  hash_b3: string;
  created_at: string;
}

export interface PolicyValidationResponse {
  valid: boolean;
  errors: string[];
  hash_b3: string | null;
}

// ===== Enhanced Inference Types =====
export interface EnhancedInferRequest {
  prompt: string;
  max_tokens?: number;
  temperature?: number;
  top_k?: number;
  top_p?: number;
  seed?: number;
  require_evidence?: boolean;
  adapters?: string[];
}

export interface EnhancedInferResponse {
  text: string;
  token_count: number;
  finish_reason: 'stop' | 'length' | 'error';
  latency_ms: number;
  trace: DetailedInferenceTrace;
}

export interface DetailedInferenceTrace {
  router_decisions: DetailedRouterDecision[];
  evidence_spans: EvidenceSpan[];
  adapter_activations: AdapterActivationDetail[];
  timeline: TraceTimelineEvent[];
  performance: TracePerformanceMetrics;
}

export interface DetailedRouterDecision {
  token_idx: number;
  adapters: string[];
  gates: number[];
  scores: number[];
  feature_vector?: FeatureVector;
  selection_reason?: string;
}

export interface AdapterActivationDetail {
  adapter_id: string;
  adapter_name: string;
  activation_count: number;
  avg_gate_value: number;
  contribution_score: number;
}

export interface TraceTimelineEvent {
  timestamp_ms: number;
  event_type: 'router' | 'evidence' | 'generation' | 'policy';
  description: string;
  duration_ms?: number;
}

export interface TracePerformanceMetrics {
  total_latency_ms: number;
  router_latency_ms: number;
  evidence_latency_ms: number;
  generation_latency_ms: number;
  tokens_per_second: number;
}

export interface InferenceSession {
  id: string;
  created_at: string;
  prompt: string;
  request: EnhancedInferRequest;
  response?: EnhancedInferResponse;
  status: 'pending' | 'running' | 'completed' | 'error';
  error_message?: string;
}

export interface InferenceComparison {
  session_a: InferenceSession;
  session_b: InferenceSession;
  differences: InferenceDifference[];
}

export interface InferenceDifference {
  field: string;
  value_a: any;
  value_b: any;
  diff_type: 'text' | 'numeric' | 'structural';
}

// ===== Policy Pack Types =====
export interface PolicyPackConfig {
  version: string;
  packs: {
    egress?: EgressRuleset;
    determinism?: DeterminismRuleset;
    router?: RouterRuleset;
    evidence?: EvidenceRuleset;
    refusal?: RefusalRuleset;
    numeric_units?: NumericUnitsRuleset;
    rag_index?: RagIndexRuleset;
    isolation?: IsolationRuleset;
    telemetry?: TelemetryRuleset;
    retention?: RetentionRuleset;
    performance?: PerformanceRuleset;
    memory?: MemoryRuleset;
    artifacts?: ArtifactsRuleset;
    secrets?: SecretsRuleset;
    build_release?: BuildReleaseRuleset;
    compliance?: ComplianceRuleset;
    incident?: IncidentRuleset;
    llm_output?: LlmOutputRuleset;
    adapter_lifecycle?: AdapterLifecycleRuleset;
    full_pack_example?: FullPackExampleRuleset;
  };
}

export interface EgressRuleset {
  allow_network_during_serving: boolean;
  enforce_uds_only: boolean;
  pf_rules_enabled: boolean;
}

export interface DeterminismRuleset {
  require_precompiled_kernels: boolean;
  hkdf_seeding_enabled: boolean;
  enforce_reproducibility: boolean;
}

export interface RouterRuleset {
  k_min: number;
  k_max: number;
  entropy_floor: number;
  gate_quantization: string;
  feature_weights: {
    language: number;
    framework: number;
    symbol_hits: number;
    path_tokens: number;
    prompt_verb: number;
  };
}

export interface EvidenceRuleset {
  require_open_book: boolean;
  min_evidence_spans: number;
  max_evidence_age_days: number;
}

export interface RefusalRuleset {
  abstain_on_low_confidence: boolean;
  confidence_threshold: number;
  refusal_message: string;
}

export interface NumericUnitsRuleset {
  normalize_units: boolean;
  validate_conversions: boolean;
  allowed_units: string[];
}

export interface RagIndexRuleset {
  per_tenant_isolation: boolean;
  deterministic_ordering: boolean;
  index_type: string;
}

export interface IsolationRuleset {
  process_per_tenant: boolean;
  uid_gid_separation: boolean;
  namespace_isolation: boolean;
}

export interface TelemetryRuleset {
  sampling_rules: {
    first_n_tokens: number;
    sampling_rate: number;
  };
  bundle_rotation_events: number;
  policy_violation_sampling: number;
}

export interface RetentionRuleset {
  bundle_retention_days: number;
  archive_after_days: number;
  purge_after_days: number;
}

export interface PerformanceRuleset {
  latency_p95_ms: number;
  latency_p99_ms: number;
  router_overhead_max_pct: number;
}

export interface MemoryRuleset {
  headroom_pct: number;
  eviction_order: string[];
  max_adapters_in_memory: number;
}

export interface ArtifactsRuleset {
  require_signature: boolean;
  require_sbom: boolean;
  verify_hash_chain: boolean;
}

export interface SecretsRuleset {
  secure_enclave_enabled: boolean;
  key_rotation_days: number;
  require_hardware_backing: boolean;
}

export interface BuildReleaseRuleset {
  determinism_gates_enabled: boolean;
  require_reproducible_builds: boolean;
  compiler_flags: string[];
}

export interface ComplianceRuleset {
  control_matrix: Record<string, string>;
  audit_log_enabled: boolean;
  retention_years: number;
}

export interface IncidentRuleset {
  runbook_paths: Record<string, string>;
  escalation_policy: string;
  on_call_rotation: string[];
}

export interface LlmOutputRuleset {
  enforce_json_format: boolean;
  require_trace: boolean;
  max_output_tokens: number;
}

export interface AdapterLifecycleRuleset {
  activation_threshold_pct: number;
  eviction_threshold_pct: number;
  category_policies: Record<AdapterCategory, CategoryPolicy>;
}

export interface FullPackExampleRuleset {
  example_field_1: string;
  example_field_2: number;
  example_nested: {
    nested_field: boolean;
  };
}

// Monitoring Types
export interface MonitoringRule {
  id: string;
  name: string;
  tenant_id: string;
  rule_type: string;
  metric_name: string;
  threshold_value: number;
  threshold_operator: string;
  evaluation_window_seconds: number;
  cooldown_seconds: number;
  severity: string;
  is_active: boolean;
  notification_channels?: Record<string, NotificationChannel>;
  escalation_rules?: Record<string, EscalationRule>;
  created_at: string;
  updated_at: string;
}

export interface CreateMonitoringRuleRequest {
  name: string;
  tenant_id: string;
  rule_type: string;
  metric_name: string;
  threshold_value: number;
  threshold_operator: string;
  evaluation_window_seconds: number;
  cooldown_seconds: number;
  severity: string;
  notification_channels?: Record<string, NotificationChannel>;
  escalation_rules?: Record<string, EscalationRule>;
}

export interface Alert {
  id: string;
  rule_id: string;
  worker_id: string;
  tenant_id: string;
  alert_type: string;
  severity: string;
  title: string;
  message: string;
  metric_value?: number;
  threshold_value?: number;
  status: string;
  acknowledged_by?: string;
  acknowledged_at?: string;
  resolved_at?: string;
  suppression_reason?: string;
  suppression_until?: string;
  escalation_level: number;
  notification_sent: boolean;
  created_at: string;
  updated_at: string;
}

export interface AlertFilters {
  tenant_id?: string;
  worker_id?: string;
  status?: string;
  severity?: string;
  limit?: number;
}

export interface AcknowledgeAlertRequest {
  acknowledged_by: string;
  notes?: string;
}

export interface HealthMetric {
  id: string;
  worker_id: string;
  tenant_id: string;
  metric_name: string;
  value: number;
  timestamp: string;
}

// Process debugging types
export interface ProcessLogFilters {
  level?: string;
  limit?: number;
  start_time?: string;
  end_time?: string;
}

export interface ProcessLog {
  id: string;
  worker_id: string;
  level: string;
  message: string;
  timestamp: string;
  metadata?: Record<string, string | number | boolean>;
}

export interface ProcessCrash {
  id: string;
  worker_id: string;
  crash_type: string;
  stack_trace: string;
  timestamp: string;
  memory_dump?: string;
}

export interface DebugSessionConfig {
  session_type: string;
  breakpoints?: string[];
  watch_variables?: string[];
  max_duration_ms?: number;
}

export interface DebugSession {
  id: string;
  worker_id: string;
  status: string;
  created_at: string;
  config: DebugSessionConfig;
}

export interface TroubleshootingStep {
  step_type: string;
  parameters?: Record<string, string | number | boolean>;
}

//! Telemetry event types for activity feed
//! 
//! # Citations
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"

export interface TelemetryEvent {
  id: string;
  timestamp: string;
  event_type: string;
  level: string;
  message: string;
  component?: string;
  tenant_id?: string;
  user_id?: string;
  trace_id?: string;
  metadata?: Record<string, string | number | boolean>;
}

export interface TroubleshootingResult {
  step_id: string;
  success: boolean;
  output: string;
  recommendations?: string[];
}

// Routing types
export interface RoutingDecisionFilters {
  limit?: number;
  adapter_id?: string;
  start_time?: string;
  end_time?: string;
  tenant?: string; // Optional tenant ID (if not provided, backend uses JWT claims)
}

export interface AdapterSelection {
  adapter_id: string;
  gate_value: number;
  rank: number;
}

export interface JourneyResponse {
  journey_type: string;
  id: string;
  data: Record<string, any>;
  states: JourneyState[];
  created_at: string;
}

export interface JourneyState {
  state: string;
  timestamp: string;
  details: Record<string, any>;
}
// Contacts
export interface Contact {
  id: string;
  name: string;
  email?: string;
  category: 'user' | 'system' | 'adapter' | 'repository' | 'external';
  role?: string;
  discovered_at: string;
  interaction_count: number;
  last_interaction?: string;
}

// Workspaces
export interface Workspace {
  id: string;
  name: string;
  description?: string;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface CreateWorkspaceRequest {
  name: string;
  description?: string;
  tenant_id?: string;
}

export interface WorkspaceMember {
  id: string;
  workspace_id: string;
  tenant_id: string;
  user_id?: string;
  role: 'owner' | 'member' | 'viewer';
  permissions_json?: string;
  added_by: string;
  added_at: string;
}

export interface WorkspaceResource {
  id: string;
  workspace_id: string;
  resource_type: 'adapter' | 'node' | 'model';
  resource_id: string;
  shared_by: string;
  shared_by_tenant_id: string;
  shared_at: string;
}

export interface AddWorkspaceMemberRequest {
  tenant_id: string;
  user_id?: string;
  role: string;
  permissions_json?: string;
}

// Messages
export interface Message {
  id: string;
  workspace_id: string;
  from_user_id: string;
  from_tenant_id: string;
  from_user_display_name?: string;
  content: string;
  thread_id?: string;
  created_at: string;
  edited_at?: string;
}

export interface CreateMessageRequest {
  content: string;
  thread_id?: string;
}

// Notifications
export interface Notification {
  id: string;
  user_id: string;
  workspace_id?: string;
  type: 'alert' | 'message' | 'mention' | 'activity' | 'system';
  target_type?: string;
  target_id?: string;
  title: string;
  content?: string;
  read_at?: string;
  created_at: string;
}

export interface NotificationSummary {
  total_count: number;
  unread_count: number;
}

// Activity Events
export interface ActivityEvent {
  id: string;
  workspace_id?: string;
  user_id: string;
  tenant_id: string;
  event_type: string;
  target_type?: string;
  target_id?: string;
  metadata_json?: string;
  created_at: string;
}

export interface RecentActivityEvent {
  id: string;
  timestamp: string;
  event_type: string;
  level: string;
  message: string;
  component?: string;
  tenant_id?: string;
  user_id?: string;
  metadata?: Record<string, unknown> | null;
}

export interface CreateActivityEventRequest {
  workspace_id?: string;
  event_type: string;
  target_type?: string;
  target_id?: string;
  metadata_json?: string;
}

// Tutorials
export interface TutorialStep {
  id: string;
  title: string;
  content: string;
  target_selector?: string;
  position?: string;
}

export interface Tutorial {
  id: string;
  title: string;
  description: string;
  steps: TutorialStep[];
  trigger?: 'manual' | 'auto' | 'on-error';
  dismissible: boolean;
  completed: boolean;
  dismissed: boolean;
  completed_at?: string;
  dismissed_at?: string;
}

export interface TutorialStatus {
  tutorial_id: string;
  completed: boolean;
  dismissed: boolean;
  completed_at?: string;
  dismissed_at?: string;
}

// Compliance Audit Types
export interface ComplianceAuditResponse {
  compliance_rate: number;
  total_controls: number;
  compliant_controls: number;
  active_violations: number;
  controls: ComplianceControl[];
  violations: PolicyViolationRecord[];
  timestamp: string;
}

export interface ComplianceControl {
  control_id: string;
  control_name: string;
  status: string;
  last_checked: string;
  evidence: string[];
  findings: string[];
}

export interface PolicyViolationRecord {
  id: string;
  reason: string;
  violation_type: string | null;
  created_at: string;
  released: boolean;
  cpid: string | null;
  metadata: string | null;
}

// Service status types - matches crates/adapteros-server/src/status_writer.rs L19-71
export interface ServiceStatus {
  id: string;
  name: string;
  state: 'stopped' | 'starting' | 'running' | 'stopping' | 'failed' | 'restarting';
  pid?: number;
  port?: number;
  health_status: 'unknown' | 'healthy' | 'unhealthy' | 'checking';
  restart_count: number;
  last_error?: string;
}

export interface AdapterOSStatus {
  schema_version?: string;
  status: 'ok' | 'degraded' | 'error';
  uptime_secs: number;
  adapters_loaded: number;
  deterministic: boolean;
  kernel_hash: string;
  telemetry_mode: string;
  worker_count: number;
  base_model_loaded?: boolean;
  base_model_id?: string;
  base_model_name?: string;
  base_model_status?: string;
  base_model_memory_mb?: number;
  services?: ServiceStatus[];
}

// Dashboard Configuration Types
export interface DashboardWidgetConfig {
  id: string;
  user_id: string;
  widget_id: string;
  enabled: boolean;
  position: number;
  created_at: string;
  updated_at: string;
}

export interface DashboardConfig {
  widgets: DashboardWidgetConfig[];
}

export interface WidgetConfigUpdate {
  widget_id: string;
  enabled: boolean;
  position: number;
}

export interface UpdateDashboardConfigRequest {
  widgets: WidgetConfigUpdate[];
}

export interface UpdateDashboardConfigResponse {
  success: boolean;
  updated_count: number;
}

export interface ResetDashboardConfigResponse {
  success: boolean;
  message: string;
}
