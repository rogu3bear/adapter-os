/**
 * Deterministic Replay Types
 *
 * TypeScript types corresponding to Rust types in:
 * crates/adapteros-server-api/src/types.rs
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

// ============================================================================
// Core Replay Types
// ============================================================================

/**
 * Sampling parameters for inference replay
 *
 * Captures all parameters that affect token generation for reproducibility.
 */
export interface SamplingParams {
  /** Sampling temperature (0.0 - 2.0) */
  temperature: number;
  /** Top-K sampling (undefined to disable) */
  top_k?: number;
  /** Top-P nucleus sampling (undefined to disable) */
  top_p?: number;
  /** Maximum tokens to generate */
  max_tokens: number;
  /** Random seed for reproducibility (undefined for non-deterministic) */
  seed?: number;
}

/**
 * Replay key containing all inputs needed for deterministic reproduction
 *
 * This is the "recipe" for recreating an inference operation exactly.
 */
export interface ReplayKey {
  /** BLAKE3 hash of the manifest used */
  manifest_hash: string;
  /** Router seed for deterministic adapter selection */
  router_seed?: string;
  /** Sampling parameters used */
  sampler_params: SamplingParams;
  /** Backend used (CoreML, MLX, Metal) */
  backend: string;
  /** Version of the sampling algorithm */
  sampling_algorithm_version: string;
  /** BLAKE3 hash of sorted RAG document hashes (undefined if no RAG) */
  rag_snapshot_hash?: string;
  /** Adapter IDs selected by router */
  adapter_ids?: string[];
  /** Whether the inference ran in base-only mode (no adapters) */
  base_only?: boolean;
}

// ============================================================================
// Enums
// ============================================================================

/**
 * Replay availability status
 */
export type ReplayStatus =
  /** Exact replay possible (all conditions match) */
  | 'available'
  /** RAG context changed but documents exist */
  | 'approximate'
  /** Some RAG documents are missing */
  | 'degraded'
  /** Critical components missing (manifest, backend) */
  | 'unavailable';

/**
 * Match status after replay execution
 */
export type ReplayMatchStatus =
  /** Token-for-token identical output */
  | 'exact'
  /** Semantically similar but not identical */
  | 'semantic'
  /** Significantly different output */
  | 'divergent'
  /** Error during replay execution */
  | 'error';

// ============================================================================
// Request Types
// ============================================================================

/**
 * Request to execute a deterministic replay
 */
export interface ReplayRequest {
  /** Inference ID to replay (lookup metadata by ID) */
  inference_id?: string;
  /** Alternatively, provide full replay key */
  replay_key?: ReplayKey;
  /** Override prompt (uses stored prompt if not provided) */
  prompt?: string;
  /** Allow approximate/degraded replay (default: false) */
  allow_approximate?: boolean;
  /** Skip RAG retrieval (test pure model determinism) */
  skip_rag?: boolean;
}

// ============================================================================
// Response Types
// ============================================================================

/**
 * RAG reproducibility details
 */
export interface RagReproducibility {
  /** Score from 0.0 (no overlap) to 1.0 (all docs available) */
  score: number;
  /** Number of original documents still available */
  matching_docs: number;
  /** Total number of documents in original inference */
  total_original_docs: number;
  /** Document IDs that are no longer available */
  missing_doc_ids: string[];
}

/**
 * Details about response divergence
 */
export interface DivergenceDetails {
  /** Character position where divergence was detected (undefined if exact match) */
  divergence_position?: number;
  /** Whether the backend changed from original */
  backend_changed: boolean;
  /** Whether the manifest hash changed */
  manifest_changed: boolean;
  /** Human-readable reasons for approximation */
  approximation_reasons: string[];
}

/**
 * Statistics from replay execution
 */
export interface ReplayStats {
  /** Number of tokens generated in replay */
  tokens_generated: number;
  /** Replay latency in milliseconds */
  latency_ms: number;
  /** Original inference latency (if recorded) */
  original_latency_ms?: number;
}

/**
 * Response from replay execution
 */
export interface ReplayResponse {
  /** Unique ID for this replay execution */
  replay_id: string;
  /** Original inference ID that was replayed */
  original_inference_id: string;
  /** Mode used for replay (exact, approximate, degraded) */
  replay_mode: string;
  /** Generated response text */
  response: string;
  /** Whether response was truncated to 64KB limit */
  response_truncated: boolean;
  /** Match status compared to original */
  match_status: ReplayMatchStatus;
  /** RAG reproducibility details (if RAG was used) */
  rag_reproducibility?: RagReproducibility;
  /** Divergence details (if not exact match) */
  divergence?: DivergenceDetails;
  /** Original response for comparison */
  original_response: string;
  /** Execution statistics */
  stats: ReplayStats;
}

/**
 * Response from checking replay availability
 */
export interface ReplayAvailabilityResponse {
  /** Inference ID checked */
  inference_id: string;
  /** Current replay status */
  status: ReplayStatus;
  /** Whether exact replay is possible */
  can_replay_exact: boolean;
  /** Whether approximate replay is possible */
  can_replay_approximate: boolean;
  /** Reasons why replay is unavailable (if applicable) */
  unavailable_reasons: string[];
  /** Warnings about approximations (if approximate) */
  approximation_warnings: string[];
  /** The replay key (if available) */
  replay_key?: ReplayKey;
}

/**
 * Single replay execution record for history
 */
export interface ReplayExecutionRecord {
  /** Replay execution ID */
  id: string;
  /** Original inference ID */
  original_inference_id: string;
  /** Mode used (exact, approximate, degraded) */
  replay_mode: string;
  /** Match status result */
  match_status: ReplayMatchStatus;
  /** RAG reproducibility score (if RAG used) */
  rag_reproducibility_score?: number;
  /** Execution timestamp (RFC3339) */
  executed_at: string;
  /** User who executed the replay */
  executed_by?: string;
  /** Error message if match_status is Error */
  error_message?: string;
}

/**
 * Response containing replay execution history
 */
export interface ReplayHistoryResponse {
  /** Original inference ID */
  inference_id: string;
  /** List of replay executions */
  executions: ReplayExecutionRecord[];
  /** Total count of executions */
  total_count: number;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Get Tailwind color class for replay status
 */
export function getReplayStatusColor(status: ReplayStatus): string {
  switch (status) {
    case 'available':
      return 'text-green-600 dark:text-green-400';
    case 'approximate':
      return 'text-yellow-600 dark:text-yellow-400';
    case 'degraded':
      return 'text-orange-600 dark:text-orange-400';
    case 'unavailable':
      return 'text-red-600 dark:text-red-400';
    default:
      return 'text-gray-600 dark:text-gray-400';
  }
}

/**
 * Get icon name for replay status
 *
 * Returns Lucide icon names (use with lucide-react)
 */
export function getReplayStatusIcon(status: ReplayStatus): string {
  switch (status) {
    case 'available':
      return 'CheckCircle2';
    case 'approximate':
      return 'AlertCircle';
    case 'degraded':
      return 'AlertTriangle';
    case 'unavailable':
      return 'XCircle';
    default:
      return 'Circle';
  }
}

/**
 * Get Tailwind color class for match status
 */
export function getMatchStatusColor(status: ReplayMatchStatus): string {
  switch (status) {
    case 'exact':
      return 'text-green-600 dark:text-green-400';
    case 'semantic':
      return 'text-blue-600 dark:text-blue-400';
    case 'divergent':
      return 'text-orange-600 dark:text-orange-400';
    case 'error':
      return 'text-red-600 dark:text-red-400';
    default:
      return 'text-gray-600 dark:text-gray-400';
  }
}

/**
 * Get icon name for match status
 *
 * Returns Lucide icon names (use with lucide-react)
 */
export function getMatchStatusIcon(status: ReplayMatchStatus): string {
  switch (status) {
    case 'exact':
      return 'CheckCircle2';
    case 'semantic':
      return 'Lightbulb';
    case 'divergent':
      return 'TrendingUp';
    case 'error':
      return 'XCircle';
    default:
      return 'Circle';
  }
}

/**
 * Get human-readable label for replay status
 */
export function getReplayStatusLabel(status: ReplayStatus): string {
  switch (status) {
    case 'available':
      return 'Available';
    case 'approximate':
      return 'Approximate';
    case 'degraded':
      return 'Degraded';
    case 'unavailable':
      return 'Unavailable';
    default:
      return 'Unknown';
  }
}

/**
 * Get human-readable label for match status
 */
export function getMatchStatusLabel(status: ReplayMatchStatus): string {
  switch (status) {
    case 'exact':
      return 'Exact Match';
    case 'semantic':
      return 'Semantic Match';
    case 'divergent':
      return 'Divergent';
    case 'error':
      return 'Error';
    default:
      return 'Unknown';
  }
}

/**
 * Calculate RAG reproducibility percentage
 */
export function getRagReproducibilityPercent(rag: RagReproducibility): number {
  return Math.round(rag.score * 100);
}

/**
 * Format latency difference
 */
export function formatLatencyDiff(stats: ReplayStats): string {
  if (!stats.original_latency_ms) {
    return `${stats.latency_ms}ms`;
  }

  const diff = stats.latency_ms - stats.original_latency_ms;
  const sign = diff > 0 ? '+' : '';
  const percent = ((diff / stats.original_latency_ms) * 100).toFixed(1);

  return `${stats.latency_ms}ms (${sign}${diff}ms, ${sign}${percent}%)`;
}

/**
 * Check if replay is possible (exact or approximate)
 */
export function canReplay(availability: ReplayAvailabilityResponse): boolean {
  return availability.can_replay_exact || availability.can_replay_approximate;
}

/**
 * Get badge variant for replay status
 *
 * Returns variant names compatible with shadcn/ui Badge component
 */
export function getReplayStatusBadgeVariant(status: ReplayStatus): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (status) {
    case 'available':
      return 'default';
    case 'approximate':
      return 'secondary';
    case 'degraded':
      return 'outline';
    case 'unavailable':
      return 'destructive';
    default:
      return 'secondary';
  }
}

/**
 * Get badge variant for match status
 *
 * Returns variant names compatible with shadcn/ui Badge component
 */
export function getMatchStatusBadgeVariant(status: ReplayMatchStatus): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (status) {
    case 'exact':
      return 'default';
    case 'semantic':
      return 'secondary';
    case 'divergent':
      return 'outline';
    case 'error':
      return 'destructive';
    default:
      return 'secondary';
  }
}
