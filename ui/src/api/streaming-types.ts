/**
 * SSE Streaming Event Types
 *
 * Strongly-typed definitions for all server-sent event (SSE) endpoints.
 * Provides structured contracts for real-time updates across the AdapterOS UI.
 *
 * Event Streams:
 * - /v1/streams/training - Training job progress events
 * - /v1/streams/discovery - Adapter discovery/search events
 * - /v1/streams/contacts - Contact/collaboration events
 * - /v1/streams/file-changes - File system change events
 * - /v1/stream/metrics - System metrics (5-sec interval)
 * - /v1/stream/telemetry - Canonical telemetry events
 * - /v1/stream/adapters - Adapter lifecycle state transitions
 */

import { Citation } from './api-types';

// ============================================================================
// Training Stream Events
// ============================================================================

/**
 * Training job progress update
 * Sent when training metrics change or milestones are reached
 */
export interface TrainingProgressEvent {
  job_id: string;
  dataset_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress_pct: number;
  current_epoch?: number;
  total_epochs?: number;
  current_loss?: number;
  validation_loss?: number;
  learning_rate?: number;
  batch_size?: number;
  tokens_processed?: number;
  tokens_per_second?: number;
  estimated_time_remaining_sec?: number;
  timestamp: string;
  error?: string;
}

/**
 * Training session state change event
 */
export interface TrainingSessionEvent {
  session_id: string;
  job_id?: string;
  action: 'created' | 'started' | 'completed' | 'cancelled';
  timestamp: string;
  user_id?: string;
  reason?: string;
}

/**
 * Training artifact ready event (weights, checkpoint, etc.)
 */
export interface TrainingArtifactEvent {
  job_id: string;
  artifact_type: 'checkpoint' | 'weights' | 'logs' | 'metrics' | 'final_model';
  artifact_id: string;
  artifact_path: string;
  size_bytes: number;
  timestamp: string;
  checksum?: string;
}

/**
 * Union of all training stream events
 */
export type TrainingStreamEvent = TrainingProgressEvent | TrainingSessionEvent | TrainingArtifactEvent;

// ============================================================================
// Discovery Stream Events
// ============================================================================

/**
 * Adapter discovered or indexed
 */
export interface AdapterDiscoveredEvent {
  adapter_id: string;
  name: string;
  version: string;
  tier: 'ephemeral' | 'warm' | 'persistent';
  rank?: number;
  alpha?: number;
  domain?: string;
  tags: string[];
  relevance_score: number;
  timestamp: string;
}

/**
 * Index update event (bulk discovery)
 */
export interface IndexUpdateEvent {
  index_id: string;
  operation: 'rebuild' | 'incremental_update' | 'optimize';
  items_indexed: number;
  duration_ms: number;
  timestamp: string;
  error?: string;
}

/**
 * Union of all discovery stream events
 */
export type DiscoveryStreamEvent = AdapterDiscoveredEvent | IndexUpdateEvent;

// ============================================================================
// Contact/Collaboration Stream Events
// ============================================================================

/**
 * Contact added or updated
 */
export interface ContactEvent {
  contact_id: string;
  action: 'added' | 'updated' | 'deleted' | 'blocked' | 'unblocked';
  name: string;
  email?: string;
  role?: 'user' | 'service' | 'bot';
  last_interaction?: string;
  timestamp: string;
}

/**
 * Collaboration invitation or permission change
 */
export interface CollaborationEvent {
  collaboration_id: string;
  resource_type: 'adapter' | 'training_job' | 'dataset' | 'workspace';
  resource_id: string;
  action: 'invited' | 'joined' | 'left' | 'permission_changed' | 'removed';
  user_id: string;
  permission_level?: 'view' | 'edit' | 'admin';
  timestamp: string;
}

/**
 * Union of all contact stream events
 */
export type ContactStreamEvent = ContactEvent | CollaborationEvent;

// ============================================================================
// File Change Stream Events
// ============================================================================

/**
 * File or directory change notification
 */
export interface FileChangeEvent {
  path: string;
  change_type: 'created' | 'modified' | 'deleted' | 'renamed' | 'permission_changed';
  is_directory: boolean;
  size_bytes?: number;
  mime_type?: string;
  previous_path?: string;
  timestamp: string;
  user_id?: string;
}

/**
 * Batch file change summary (for high-frequency updates)
 */
export interface FileChangeBatchEvent {
  batch_id: string;
  changes: FileChangeEvent[];
  total_changes: number;
  timestamp: string;
}

/**
 * Union of all file change stream events
 */
export type FileChangeStreamEvent = FileChangeEvent | FileChangeBatchEvent;

// ============================================================================
// System Metrics Stream Events
// ============================================================================

/**
 * Backend SSE metrics snapshot event
 * Matches MetricsSnapshotEvent from crates/adapteros-server-api/src/handlers/streaming.rs
 */
export interface MetricsSnapshotEvent {
  timestamp_ms: number;
  latency: {
    p50_ms: number;
    p95_ms: number;
    p99_ms: number;
  };
  throughput: {
    tokens_per_second: number;
    inferences_per_second: number;
  };
  system: {
    cpu_percent: number;
    memory_percent: number;
    disk_percent: number;
  };
}

/**
 * System metrics snapshot (5-sec interval)
 * Legacy/extended format - kept for compatibility
 */
export interface SystemMetricsEvent {
  timestamp: string;
  cpu: {
    usage_percent: number;
    cores: number;
    temp_celsius?: number;
  };
  memory: {
    used_gb: number;
    total_gb: number;
    usage_percent: number;
  };
  disk: {
    used_gb: number;
    total_gb: number;
    usage_percent: number;
    read_mbps?: number;
    write_mbps?: number;
  };
  network: {
    rx_bytes?: number;
    tx_bytes?: number;
    rx_packets?: number;
    tx_packets?: number;
  };
  gpu?: {
    utilization_percent?: number;
    memory_used_mb?: number;
    memory_total_mb?: number;
    temp_celsius?: number;
    power_watts?: number;
  };
}

/**
 * Performance degradation alert
 */
export interface PerformanceAlertEvent {
  alert_id: string;
  severity: 'warning' | 'critical';
  metric: 'cpu' | 'memory' | 'disk' | 'network' | 'gpu';
  threshold_value: number;
  current_value: number;
  recommendation?: string;
  timestamp: string;
}

/**
 * Union of all metrics stream events
 */
export type MetricsStreamEvent = MetricsSnapshotEvent | SystemMetricsEvent | PerformanceAlertEvent;

// ============================================================================
// Telemetry Stream Events
// ============================================================================

/**
 * Streaming telemetry event (structured logging for SSE streams)
 * Note: This is distinct from TelemetryEvent in api-types.ts which matches the backend canonical format
 */
export interface StreamingTelemetryEvent {
  event_id: string;
  event_type: string;
  correlation_id?: string;
  user_id?: string;
  tenant_id?: string;
  resource_type?: string;
  resource_id?: string;
  action: string;
  status: 'success' | 'failure' | 'pending';
  duration_ms?: number;
  metadata: Record<string, unknown>;
  timestamp: string;
  signature?: string;
}

/**
 * Batched telemetry events
 */
export interface TelemetryBatchEvent {
  batch_id: string;
  events: StreamingTelemetryEvent[];
  timestamp: string;
}

/**
 * Union of all telemetry stream events
 */
export type TelemetryStreamEvent = StreamingTelemetryEvent | TelemetryBatchEvent;

// ============================================================================
// Adapter Stream Events
// ============================================================================

/**
 * Adapter lifecycle state transition
 * Matches AdapterStateEvent from crates/adapteros-server-api/src/handlers/streaming.rs
 */
export interface AdapterStateTransitionEvent {
  adapter_id: string;
  adapter_name: string;
  previous_state: string | null;
  current_state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  /** Alias for current_state - some API responses use this field name */
  new_state?: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  timestamp: number; // Unix timestamp in ms
  activation_percentage: number;
  memory_usage_mb?: number;
}

/**
 * Adapter performance metric update
 */
export interface AdapterMetricsEvent {
  adapter_id: string;
  tenant_id: string;
  inference_count: number;
  avg_latency_ms: number;
  p99_latency_ms: number;
  success_rate_pct: number;
  error_rate_pct: number;
  tokens_per_second?: number;
  timestamp: string;
}

/**
 * Adapter error or health issue
 */
export interface AdapterHealthEvent {
  adapter_id: string;
  tenant_id: string;
  status: 'healthy' | 'degraded' | 'unhealthy' | 'error';
  issue?: string;
  recommendation?: string;
  timestamp: string;
  auto_remediation_attempted?: boolean;
}

/**
 * Adapter pinning status change
 */
export interface AdapterPinEvent {
  adapter_id: string;
  tenant_id: string;
  action: 'pinned' | 'unpinned';
  expires_at?: string;
  reason?: string;
  pinned_by?: string;
  timestamp: string;
}

/**
 * Union of all adapter stream events
 */
export type AdapterStreamEvent =
  | AdapterStateTransitionEvent
  | AdapterMetricsEvent
  | AdapterHealthEvent
  | AdapterPinEvent;

// ============================================================================
// Base Stream Event Type
// ============================================================================

/**
 * Discriminated union of all SSE event types
 */
export type StreamEvent =
  | { type: 'training'; data: TrainingStreamEvent }
  | { type: 'discovery'; data: DiscoveryStreamEvent }
  | { type: 'contacts'; data: ContactStreamEvent }
  | { type: 'file_changes'; data: FileChangeStreamEvent }
  | { type: 'metrics'; data: MetricsStreamEvent }
  | { type: 'telemetry'; data: TelemetryStreamEvent }
  | { type: 'adapters'; data: AdapterStreamEvent };

/**
 * Raw SSE message as received from server
 */
export interface RawSSEMessage {
  id?: string;
  event?: string;
  data: string;
  retry?: number;
}

/**
 * SSE stream state for components
 */
export interface StreamState<T> {
  data: T | null;
  error: string | null;
  connected: boolean;
  reconnect: () => void;
  lastUpdated?: string;
}

// ============================================================================
// Stream Configuration
// ============================================================================

/**
 * Configuration for stream subscriptions
 */
export interface StreamConfig {
  /** Enable/disable the stream */
  enabled?: boolean;

  /** Callback when event is received */
  onMessage?: <T = unknown>(data: T) => void;

  /** Callback on stream error */
  onError?: (error: Event) => void;

  /** Callback on connection open */
  onOpen?: () => void;

  /** Callback on connection close */
  onClose?: () => void;

  /** Auto-reconnect on disconnect */
  autoReconnect?: boolean;

  /** Max reconnection attempts */
  maxReconnectAttempts?: number;

  /** Initial backoff delay in ms */
  initialBackoffMs?: number;

  /** Max backoff delay in ms */
  maxBackoffMs?: number;
}

// ============================================================================
// Stream Utilities
// ============================================================================

/**
 * Helper to safely parse SSE event data
 */
export function parseStreamEvent<T = unknown>(data: string): T {
  try {
    return JSON.parse(data) as T;
  } catch (e) {
    const errorMessage = e instanceof Error ? e.message : String(e);
    throw new Error(`Failed to parse stream event: ${errorMessage}`);
  }
}

/**
 * Helper to check if event is a training progress event
 */
export function isTrainingProgressEvent(data: unknown): data is TrainingProgressEvent {
  return typeof data === 'object' && data !== null && 'job_id' in data && 'progress_pct' in data;
}

/**
 * Helper to check if event is an adapter state transition
 */
export function isAdapterStateTransitionEvent(data: unknown): data is AdapterStateTransitionEvent {
  return typeof data === 'object' && data !== null && 'adapter_id' in data && 'current_state' in data;
}

/**
 * Helper to check if event is system metrics
 */
export function isSystemMetricsEvent(data: unknown): data is SystemMetricsEvent {
  return typeof data === 'object' && data !== null && 'cpu' in data && 'memory' in data && 'disk' in data;
}

// ============================================================================
// Streaming Inference Types (OpenAI-compatible)
// ============================================================================

/**
 * Request payload for streaming inference endpoint
 * POST /v1/infer (with stream: true)
 */
export interface StreamingInferRequest {
  /** The prompt text to generate from */
  prompt: string;

  /** Model identifier (optional, uses default if not specified) */
  model?: string;

  /** Backend selection (mlx, coreml, metal, or auto) */
  backend?: 'mlx' | 'coreml' | 'metal' | 'auto';

  /** Per-request override for router determinism (deterministic/adaptive) */
  routing_determinism_mode?: string;

  /** Maximum number of tokens to generate */
  max_tokens?: number;

  /** Sampling temperature (0.0-2.0, higher = more creative) */
  temperature?: number;

  /** Top-p (nucleus) sampling parameter */
  top_p?: number;

  /** Top-k sampling parameter */
  top_k?: number;

  /** Stop sequences to halt generation */
  stop?: string[];

  /** Adapter stack to use for inference (array of adapter IDs) */
  adapter_stack?: string[] | string;

  /** Named adapter stack identifier */
  stack_id?: string;

  /** Optional domain hint to bias routing */
  domain?: string;

  /** Per-adapter strength overrides (multipliers, default 1.0) */
  adapter_strength_overrides?: Record<string, number>;

  /** Collection ID for RAG-enhanced inference */
  collection_id?: string;

  /** Random seed for reproducible generation */
  seed?: number;
}

/**
 * A single streaming chunk response (OpenAI chat.completion.chunk format)
 * Delivered via SSE: data: {"id":"...","object":"chat.completion.chunk",...}
 */
export interface StreamingChunk {
  /** Unique identifier for the completion */
  id: string;

  /** Object type, always "chat.completion.chunk" for streaming */
  object: string;

  /** Unix timestamp of when the chunk was created */
  created: number;

  /** Model used for generation */
  model: string;

  /** Array of choices (typically one for streaming) */
  choices: StreamingChoice[];
}

/**
 * A single choice within a streaming chunk
 */
export interface StreamingChoice {
  /** Index of this choice in the choices array */
  index: number;

  /** Incremental content delta */
  delta: StreamingDelta;

  /** Reason for stopping (null while streaming, set on final chunk) */
  finish_reason: 'stop' | 'length' | 'content_filter' | null;
}

/**
 * Delta content within a streaming choice
 */
export interface StreamingDelta {
  /** Role of the message (only present in first chunk) */
  role?: 'assistant' | 'user' | 'system';

  /** Incremental text content */
  content?: string;
}

/**
 * Helper to check if data is a streaming inference chunk
 */
export function isStreamingChunk(data: unknown): data is StreamingChunk {
  return typeof data === 'object' && data !== null && 'object' in data && (data as StreamingChunk).object === 'chat.completion.chunk' && 'choices' in data;
}

// ============================================================================
// Stack Policy Stream Events
// Endpoint: /v1/stream/stack-policies/{stack_id}
// ============================================================================

/**
 * Event types emitted by the stack policy stream
 */
export type StackPolicyEventType =
  | 'compliance_changed'
  | 'violation_detected'
  | 'violation_resolved'
  | 'policy_assigned'
  | 'policy_revoked';

/**
 * Compliance score changed event
 */
export interface ComplianceChangedEvent {
  event_type: 'compliance_changed';
  stack_id: string;
  previous_score: number;
  current_score: number;
  previous_status: 'compliant' | 'warning' | 'non_compliant';
  current_status: 'compliant' | 'warning' | 'non_compliant';
  changed_categories: string[];
  timestamp: string;
}

/**
 * Policy violation detected event
 */
export interface ViolationDetectedEvent {
  event_type: 'violation_detected';
  stack_id: string;
  violation_id: string;
  policy_pack_id: string;
  policy_name: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
  message: string;
  resource_type: string;
  resource_id: string;
  timestamp: string;
}

/**
 * Policy violation resolved event
 */
export interface ViolationResolvedEvent {
  event_type: 'violation_resolved';
  stack_id: string;
  violation_id: string;
  policy_pack_id: string;
  policy_name: string;
  resolved_by: string;
  resolution_notes?: string;
  timestamp: string;
}

/**
 * Policy assigned to stack event
 */
export interface PolicyAssignedEvent {
  event_type: 'policy_assigned';
  stack_id: string;
  assignment_id: string;
  policy_pack_id: string;
  policy_name: string;
  assigned_by: string;
  priority: number;
  enforced: boolean;
  timestamp: string;
}

/**
 * Policy revoked from stack event
 */
export interface PolicyRevokedEvent {
  event_type: 'policy_revoked';
  stack_id: string;
  assignment_id: string;
  policy_pack_id: string;
  policy_name: string;
  revoked_by: string;
  reason?: string;
  timestamp: string;
}

/**
 * Union of all stack policy stream events
 */
export type StackPolicyStreamEvent =
  | ComplianceChangedEvent
  | ViolationDetectedEvent
  | ViolationResolvedEvent
  | PolicyAssignedEvent
  | PolicyRevokedEvent;

/**
 * Helper to check if event is a compliance change
 */
export function isComplianceChangedEvent(data: unknown): data is ComplianceChangedEvent {
  return typeof data === 'object' && data !== null && 'event_type' in data && (data as ComplianceChangedEvent).event_type === 'compliance_changed';
}

/**
 * Helper to check if event is a violation detected
 */
export function isViolationDetectedEvent(data: unknown): data is ViolationDetectedEvent {
  return typeof data === 'object' && data !== null && 'event_type' in data && (data as ViolationDetectedEvent).event_type === 'violation_detected';
}

/**
 * Helper to check if event is a violation resolved
 */
export function isViolationResolvedEvent(data: unknown): data is ViolationResolvedEvent {
  return typeof data === 'object' && data !== null && 'event_type' in data && (data as ViolationResolvedEvent).event_type === 'violation_resolved';
}

// ============================================================================
// Boot Progress Stream Events
// Endpoint: /v1/stream/boot-progress
// ============================================================================

/**
 * Loading phase during boot process
 */
export type BootLoadingPhase = 'initializing' | 'downloading' | 'loading' | 'ready';

/**
 * Boot state change event
 * Emitted when system transitions between boot states
 */
export interface StateChangedEvent {
  event_type: 'StateChanged';
  previous: string;
  current: string;
  elapsed_ms: number;
  models_pending: number;
  models_ready: number;
}

/**
 * Model download progress event
 * Emitted during model downloads from HuggingFace registry
 */
export interface DownloadProgressEvent {
  event_type: 'DownloadProgress';
  model_id: string;
  repo_id: string;
  downloaded_bytes: number;
  total_bytes: number;
  speed_mbps: number;
  eta_seconds: number;
  files_completed: number;
  files_total: number;
}

/**
 * Model loading progress event
 * Emitted during model weight loading and backend initialization
 */
export interface LoadProgressEvent {
  event_type: 'LoadProgress';
  model_id: string;
  phase: BootLoadingPhase;
  progress_pct: number;
  memory_allocated_mb: number;
}

/**
 * Model ready event
 * Emitted when a model completes loading and warmup
 */
export interface ModelReadyEvent {
  event_type: 'ModelReady';
  model_id: string;
  warmup_latency_ms: number;
  memory_usage_mb: number;
}

/**
 * Fully ready event
 * Emitted when all models are loaded and system is ready
 */
export interface FullyReadyEvent {
  event_type: 'FullyReady';
  total_models: number;
  total_download_mb: number;
  total_load_time_ms: number;
}

/**
 * Union of all boot progress stream events
 */
export type BootProgressEvent =
  | StateChangedEvent
  | DownloadProgressEvent
  | LoadProgressEvent
  | ModelReadyEvent
  | FullyReadyEvent;

/**
 * Helper to check if event is a state change
 */
export function isStateChangedEvent(data: unknown): data is StateChangedEvent {
  return typeof data === 'object' && data !== null && 'event_type' in data && (data as StateChangedEvent).event_type === 'StateChanged';
}

/**
 * Helper to check if event is download progress
 */
export function isDownloadProgressEvent(data: unknown): data is DownloadProgressEvent {
  return typeof data === 'object' && data !== null && 'event_type' in data && (data as DownloadProgressEvent).event_type === 'DownloadProgress';
}

/**
 * Helper to check if event is load progress
 */
export function isLoadProgressEvent(data: unknown): data is LoadProgressEvent {
  return typeof data === 'object' && data !== null && 'event_type' in data && (data as LoadProgressEvent).event_type === 'LoadProgress';
}

/**
 * Helper to check if event is model ready
 */
export function isModelReadyEvent(data: unknown): data is ModelReadyEvent {
  return typeof data === 'object' && data !== null && 'event_type' in data && (data as ModelReadyEvent).event_type === 'ModelReady';
}

/**
 * Helper to check if event is fully ready
 */
export function isFullyReadyEvent(data: unknown): data is FullyReadyEvent {
  return typeof data === 'object' && data !== null && 'event_type' in data && (data as FullyReadyEvent).event_type === 'FullyReady';
}

// ============================================================================
// Inference Stream Events (with Loading Progress)
// Endpoint: /v1/infer/stream/progress
// ============================================================================

/**
 * Load phases for model loading progress
 * Matches LoadPhase enum from streaming_infer.rs
 */
export type LoadPhase = 'Downloading' | 'LoadingWeights' | 'Warmup';

/**
 * Inference event types for progress streaming
 * Matches InferenceEvent enum from crates/adapteros-server-api/src/handlers/streaming_infer.rs
 * Uses 'event' as discriminant field (from #[serde(tag = "event")])
 */
export type InferenceEvent =
  | { event: 'Loading'; phase: LoadPhase; progress: number; eta_seconds?: number }
  | { event: 'Ready'; warmup_latency_ms: number }
  | { event: 'Token'; text: string; token_id?: number }
  | { event: 'Done'; total_tokens: number; latency_ms: number; unavailable_pinned_adapters?: string[]; pinned_routing_fallback?: string; citations?: Citation[] }
  | { event: 'Error'; message: string; recoverable: boolean };

/**
 * Type guard for InferenceEvent
 * Validates that the object has an 'event' field matching one of the valid event types
 */
export function isInferenceEvent(obj: unknown): obj is InferenceEvent {
  if (typeof obj !== 'object' || obj === null || !('event' in obj)) {
    return false;
  }
  const eventType = (obj as { event: string }).event;
  return ['Loading', 'Ready', 'Token', 'Done', 'Error'].includes(eventType);
}

/**
 * Helper to check if event is a Loading event
 */
export function isInferenceLoadingEvent(data: unknown): data is Extract<InferenceEvent, { event: 'Loading' }> {
  return typeof data === 'object' && data !== null && 'event' in data && (data as InferenceEvent).event === 'Loading';
}

/**
 * Helper to check if event is a Ready event
 */
export function isInferenceReadyEvent(data: unknown): data is Extract<InferenceEvent, { event: 'Ready' }> {
  return typeof data === 'object' && data !== null && 'event' in data && (data as InferenceEvent).event === 'Ready';
}

/**
 * Helper to check if event is a Token event
 */
export function isInferenceTokenEvent(data: unknown): data is Extract<InferenceEvent, { event: 'Token' }> {
  return typeof data === 'object' && data !== null && 'event' in data && (data as InferenceEvent).event === 'Token';
}

/**
 * Helper to check if event is a Done event
 */
export function isInferenceDoneEvent(data: unknown): data is Extract<InferenceEvent, { event: 'Done' }> {
  return typeof data === 'object' && data !== null && 'event' in data && (data as InferenceEvent).event === 'Done';
}

/**
 * Helper to check if event is an Error event
 */
export function isInferenceErrorEvent(data: unknown): data is Extract<InferenceEvent, { event: 'Error' }> {
  return typeof data === 'object' && data !== null && 'event' in data && (data as InferenceEvent).event === 'Error';
}
