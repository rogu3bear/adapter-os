/**
 * Streaming Module - Central export point
 *
 * This module re-exports all streaming-related functionality for easier imports.
 *
 * Usage:
 * ```typescript
 * // Instead of:
 * import { streamingService } from '../services/StreamingService';
 * import { useTrainingStream } from '../hooks/useStreamingEndpoints';
 * import { TrainingStreamEvent } from '../api/streaming-types';
 *
 * // Use:
 * import { streamingService, useTrainingStream, TrainingStreamEvent } from '../streaming';
 * ```
 */

// ============================================================================
// Services
// ============================================================================
export { streamingService, default as StreamingService } from '../services/StreamingService';
export type { StreamSubscription } from '../services/StreamingService';

// ============================================================================
// Hooks
// ============================================================================
export {
  useTrainingStream,
  useDiscoveryStream,
  useContactsStream,
  useFileChangesStream,
  useMetricsStream,
  useTelemetryStream,
  useAdaptersStream,
  useAllStreamsStatus,
} from '../hooks/useStreamingEndpoints';
export type { StreamHookResult } from '../hooks/useStreamingEndpoints';

export { useSSE } from '../hooks/useSSE';
export type { UseSSEOptions } from '../hooks/useSSE';

// ============================================================================
// Types
// ============================================================================
export type {
  // Training
  TrainingProgressEvent,
  TrainingSessionEvent,
  TrainingArtifactEvent,
  TrainingStreamEvent,
  // Discovery
  AdapterDiscoveredEvent,
  IndexUpdateEvent,
  DiscoveryStreamEvent,
  // Contacts
  ContactEvent,
  CollaborationEvent,
  ContactStreamEvent,
  // File Changes
  FileChangeEvent,
  FileChangeBatchEvent,
  FileChangeStreamEvent,
  // Metrics
  SystemMetricsEvent,
  PerformanceAlertEvent,
  MetricsStreamEvent,
  // Telemetry
  StreamingTelemetryEvent,
  TelemetryBatchEvent,
  TelemetryStreamEvent,
  // Adapters
  AdapterStateTransitionEvent,
  AdapterMetricsEvent,
  AdapterHealthEvent,
  AdapterPinEvent,
  AdapterStreamEvent,
  // Base types
  StreamEvent,
  RawSSEMessage,
  StreamState,
  StreamConfig,
} from '../api/streaming-types';

// ============================================================================
// Utilities
// ============================================================================
export {
  parseStreamEvent,
  isTrainingProgressEvent,
  isAdapterStateTransitionEvent,
  isSystemMetricsEvent,
} from '../api/streaming-types';

// ============================================================================
// Components
// ============================================================================
export { StreamingIntegration } from '../components/StreamingIntegration';
