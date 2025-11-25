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
  action: 'created' | 'started' | 'paused' | 'resumed' | 'completed' | 'cancelled';
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
 * System metrics snapshot (5-sec interval)
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
export type MetricsStreamEvent = SystemMetricsEvent | PerformanceAlertEvent;

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
  metadata: Record<string, any>;
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
 */
export interface AdapterStateTransitionEvent {
  adapter_id: string;
  tenant_id: string;
  previous_state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  new_state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  trigger: 'activation' | 'eviction' | 'manual' | 'timeout' | 'memory_pressure' | 'pinning';
  memory_freed_mb?: number;
  timestamp: string;
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
  onMessage?: <T = any>(data: T) => void;

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
export function parseStreamEvent<T = any>(data: string): T {
  try {
    return JSON.parse(data) as T;
  } catch (e) {
    throw new Error(`Failed to parse stream event: ${e}`);
  }
}

/**
 * Helper to check if event is a training progress event
 */
export function isTrainingProgressEvent(data: any): data is TrainingProgressEvent {
  return data && 'job_id' in data && 'progress_pct' in data;
}

/**
 * Helper to check if event is an adapter state transition
 */
export function isAdapterStateTransitionEvent(data: any): data is AdapterStateTransitionEvent {
  return data && 'previous_state' in data && 'new_state' in data;
}

/**
 * Helper to check if event is system metrics
 */
export function isSystemMetricsEvent(data: any): data is SystemMetricsEvent {
  return data && 'cpu' in data && 'memory' in data && 'disk' in data;
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
export function isStreamingChunk(data: any): data is StreamingChunk {
  return data && 'object' in data && data.object === 'chat.completion.chunk' && 'choices' in data;
}
