import { describe, it, expect, expectTypeOf } from 'vitest';
import {
  // Type guards
  isTrainingProgressEvent,
  isAdapterStateTransitionEvent,
  isSystemMetricsEvent,
  isStreamingChunk,
  isComplianceChangedEvent,
  isViolationDetectedEvent,
  isViolationResolvedEvent,
  isStateChangedEvent,
  isDownloadProgressEvent,
  isLoadProgressEvent,
  isModelReadyEvent,
  isFullyReadyEvent,
  isInferenceEvent,
  isInferenceLoadingEvent,
  isInferenceReadyEvent,
  isInferenceTokenEvent,
  isInferenceDoneEvent,
  isInferenceErrorEvent,
  isSessionProgressEvent,
  isDatasetProgressEvent,
  isIngestionComplete,
  isIngestionFailed,
  isSseStreamDisconnectedEvent,
  isSseBufferOverflowEvent,
  isSseEventGapDetectedEvent,
  isSseHeartbeatEvent,
  isSseErrorEvent,
  getGapRecoveryAction,
  parseStreamEvent,
  // Types
  type SseGapRecoveryHint,
  type SseErrorEvent,
  type SseStreamDisconnectedEvent,
  type SseBufferOverflowEvent,
  type SseEventGapDetectedEvent,
  type SseHeartbeatEvent,
  type StreamConfig,
  type StreamState,
  type TrainingProgressEvent,
  type AdapterStateTransitionEvent,
  type InferenceEvent,
  type SessionProgressEvent,
  type IngestionPhase,
} from '../streaming-types';

// ============================================================================
// SseGapRecoveryHint Type Guards
// ============================================================================

describe('SseGapRecoveryHint', () => {
  describe('type discrimination', () => {
    it('correctly identifies refetch_full_state hint', () => {
      const hint: SseGapRecoveryHint = { type: 'refetch_full_state' };
      expect(hint.type).toBe('refetch_full_state');
      if (hint.type === 'refetch_full_state') {
        expectTypeOf(hint).toEqualTypeOf<{ type: 'refetch_full_state' }>();
      }
    });

    it('correctly identifies continue_with_gap hint', () => {
      const hint: SseGapRecoveryHint = { type: 'continue_with_gap' };
      expect(hint.type).toBe('continue_with_gap');
      if (hint.type === 'continue_with_gap') {
        expectTypeOf(hint).toEqualTypeOf<{ type: 'continue_with_gap' }>();
      }
    });

    it('correctly identifies restart_stream hint', () => {
      const hint: SseGapRecoveryHint = { type: 'restart_stream' };
      expect(hint.type).toBe('restart_stream');
      if (hint.type === 'restart_stream') {
        expectTypeOf(hint).toEqualTypeOf<{ type: 'restart_stream' }>();
      }
    });

    it('correctly identifies refetch_resource hint with resource data', () => {
      const hint: SseGapRecoveryHint = {
        type: 'refetch_resource',
        resource_type: 'adapter',
        resource_id: 'adapter-123',
      };
      expect(hint.type).toBe('refetch_resource');
      if (hint.type === 'refetch_resource') {
        expect(hint.resource_type).toBe('adapter');
        expect(hint.resource_id).toBe('adapter-123');
        expectTypeOf(hint).toEqualTypeOf<{
          type: 'refetch_resource';
          resource_type: string;
          resource_id: string;
        }>();
      }
    });
  });

  describe('getGapRecoveryAction', () => {
    it('returns refetch action for refetch_full_state', () => {
      const hint: SseGapRecoveryHint = { type: 'refetch_full_state' };
      const action = getGapRecoveryAction(hint);
      expect(action).toEqual({
        action: 'refetch',
        message: 'Full state refresh required',
        requiresRefresh: true,
      });
    });

    it('returns continue action for continue_with_gap', () => {
      const hint: SseGapRecoveryHint = { type: 'continue_with_gap' };
      const action = getGapRecoveryAction(hint);
      expect(action).toEqual({
        action: 'continue',
        message: 'Some events were missed, but you can continue',
        requiresRefresh: false,
      });
    });

    it('returns restart action for restart_stream', () => {
      const hint: SseGapRecoveryHint = { type: 'restart_stream' };
      const action = getGapRecoveryAction(hint);
      expect(action).toEqual({
        action: 'restart',
        message: 'Stream restart required',
        requiresRefresh: true,
      });
    });

    it('returns refetch_resource action with resource details', () => {
      const hint: SseGapRecoveryHint = {
        type: 'refetch_resource',
        resource_type: 'training_job',
        resource_id: 'job-456',
      };
      const action = getGapRecoveryAction(hint);
      expect(action).toEqual({
        action: 'refetch_resource',
        message: 'Refresh training_job: job-456',
        requiresRefresh: true,
      });
    });

    it('handles all hint types exhaustively (compile-time check)', () => {
      // This test ensures all SseGapRecoveryHint types are handled in the switch
      const hints: SseGapRecoveryHint[] = [
        { type: 'refetch_full_state' },
        { type: 'continue_with_gap' },
        { type: 'restart_stream' },
        { type: 'refetch_resource', resource_type: 'adapter', resource_id: 'id' },
      ];

      hints.forEach((hint) => {
        // Should not throw for any valid hint type
        expect(() => getGapRecoveryAction(hint)).not.toThrow();
      });
    });
  });
});

// ============================================================================
// SSE Error Event Type Narrowing
// ============================================================================

describe('SSE Error Event Type Narrowing', () => {
  describe('isSseStreamDisconnectedEvent', () => {
    it('returns true for valid stream disconnected event', () => {
      const event: SseStreamDisconnectedEvent = {
        type: 'stream_disconnected',
        last_event_id: 42,
        reason: 'Server shutdown',
        reconnect_hint_ms: 5000,
      };
      expect(isSseStreamDisconnectedEvent(event)).toBe(true);
    });

    it('returns false for other event types', () => {
      expect(isSseStreamDisconnectedEvent({ type: 'buffer_overflow' })).toBe(false);
      expect(isSseStreamDisconnectedEvent({ type: 'event_gap' })).toBe(false);
      expect(isSseStreamDisconnectedEvent({ type: 'heartbeat' })).toBe(false);
    });

    it('returns false for null and undefined', () => {
      expect(isSseStreamDisconnectedEvent(null)).toBe(false);
      expect(isSseStreamDisconnectedEvent(undefined)).toBe(false);
    });

    it('returns false for non-objects', () => {
      expect(isSseStreamDisconnectedEvent('string')).toBe(false);
      expect(isSseStreamDisconnectedEvent(123)).toBe(false);
      expect(isSseStreamDisconnectedEvent(true)).toBe(false);
    });
  });

  describe('isSseBufferOverflowEvent', () => {
    it('returns true for valid buffer overflow event', () => {
      const event: SseBufferOverflowEvent = {
        type: 'buffer_overflow',
        dropped_count: 15,
        oldest_available_id: 100,
      };
      expect(isSseBufferOverflowEvent(event)).toBe(true);
    });

    it('returns false for other event types', () => {
      expect(isSseBufferOverflowEvent({ type: 'stream_disconnected' })).toBe(false);
      expect(isSseBufferOverflowEvent({ type: 'event_gap' })).toBe(false);
      expect(isSseBufferOverflowEvent({ type: 'heartbeat' })).toBe(false);
    });

    it('returns false for invalid input', () => {
      expect(isSseBufferOverflowEvent(null)).toBe(false);
      expect(isSseBufferOverflowEvent({})).toBe(false);
    });
  });

  describe('isSseEventGapDetectedEvent', () => {
    it('returns true for valid event gap detected event', () => {
      const event: SseEventGapDetectedEvent = {
        type: 'event_gap',
        client_last_id: 50,
        server_oldest_id: 75,
        events_lost: 25,
        recovery_hint: { type: 'refetch_full_state' },
      };
      expect(isSseEventGapDetectedEvent(event)).toBe(true);
    });

    it('returns false for other event types', () => {
      expect(isSseEventGapDetectedEvent({ type: 'stream_disconnected' })).toBe(false);
      expect(isSseEventGapDetectedEvent({ type: 'buffer_overflow' })).toBe(false);
      expect(isSseEventGapDetectedEvent({ type: 'heartbeat' })).toBe(false);
    });

    it('validates event with all recovery hint types', () => {
      const hints: SseGapRecoveryHint[] = [
        { type: 'refetch_full_state' },
        { type: 'continue_with_gap' },
        { type: 'restart_stream' },
        { type: 'refetch_resource', resource_type: 'adapter', resource_id: 'id' },
      ];

      hints.forEach((recovery_hint) => {
        const event: SseEventGapDetectedEvent = {
          type: 'event_gap',
          client_last_id: 10,
          server_oldest_id: 20,
          events_lost: 10,
          recovery_hint,
        };
        expect(isSseEventGapDetectedEvent(event)).toBe(true);
      });
    });
  });

  describe('isSseHeartbeatEvent', () => {
    it('returns true for valid heartbeat event', () => {
      const event: SseHeartbeatEvent = {
        type: 'heartbeat',
        current_id: 123,
        timestamp_ms: Date.now(),
      };
      expect(isSseHeartbeatEvent(event)).toBe(true);
    });

    it('returns false for other event types', () => {
      expect(isSseHeartbeatEvent({ type: 'stream_disconnected' })).toBe(false);
      expect(isSseHeartbeatEvent({ type: 'buffer_overflow' })).toBe(false);
      expect(isSseHeartbeatEvent({ type: 'event_gap' })).toBe(false);
    });
  });

  describe('isSseErrorEvent (union type guard)', () => {
    it('returns true for all SSE error event types', () => {
      const events: SseErrorEvent[] = [
        {
          type: 'stream_disconnected',
          last_event_id: 1,
          reason: 'test',
          reconnect_hint_ms: 1000,
        },
        {
          type: 'buffer_overflow',
          dropped_count: 5,
          oldest_available_id: 10,
        },
        {
          type: 'event_gap',
          client_last_id: 5,
          server_oldest_id: 10,
          events_lost: 5,
          recovery_hint: { type: 'continue_with_gap' },
        },
        {
          type: 'heartbeat',
          current_id: 100,
          timestamp_ms: Date.now(),
        },
      ];

      events.forEach((event) => {
        expect(isSseErrorEvent(event)).toBe(true);
      });
    });

    it('returns false for non-SSE error events', () => {
      expect(isSseErrorEvent({ type: 'unknown' })).toBe(false);
      expect(isSseErrorEvent({ event: 'Token', text: 'hello' })).toBe(false);
      expect(isSseErrorEvent(null)).toBe(false);
      expect(isSseErrorEvent(undefined)).toBe(false);
      expect(isSseErrorEvent({})).toBe(false);
    });
  });

  describe('type narrowing in conditionals', () => {
    it('narrows SseErrorEvent union based on type field', () => {
      const handleEvent = (event: SseErrorEvent): string => {
        switch (event.type) {
          case 'stream_disconnected':
            // TypeScript should know this is SseStreamDisconnectedEvent
            return `Disconnected: ${event.reason}, reconnect in ${event.reconnect_hint_ms}ms`;
          case 'buffer_overflow':
            // TypeScript should know this is SseBufferOverflowEvent
            return `Overflow: ${event.dropped_count} events dropped`;
          case 'event_gap':
            // TypeScript should know this is SseEventGapDetectedEvent
            return `Gap: ${event.events_lost} events lost, hint: ${event.recovery_hint.type}`;
          case 'heartbeat':
            // TypeScript should know this is SseHeartbeatEvent
            return `Heartbeat: id=${event.current_id}`;
        }
      };

      const disconnected: SseStreamDisconnectedEvent = {
        type: 'stream_disconnected',
        last_event_id: 10,
        reason: 'Server restart',
        reconnect_hint_ms: 3000,
      };
      expect(handleEvent(disconnected)).toBe('Disconnected: Server restart, reconnect in 3000ms');

      const overflow: SseBufferOverflowEvent = {
        type: 'buffer_overflow',
        dropped_count: 20,
        oldest_available_id: 50,
      };
      expect(handleEvent(overflow)).toBe('Overflow: 20 events dropped');

      const gap: SseEventGapDetectedEvent = {
        type: 'event_gap',
        client_last_id: 10,
        server_oldest_id: 30,
        events_lost: 20,
        recovery_hint: { type: 'restart_stream' },
      };
      expect(handleEvent(gap)).toBe('Gap: 20 events lost, hint: restart_stream');

      const heartbeat: SseHeartbeatEvent = {
        type: 'heartbeat',
        current_id: 500,
        timestamp_ms: 1234567890,
      };
      expect(handleEvent(heartbeat)).toBe('Heartbeat: id=500');
    });
  });
});

// ============================================================================
// Event Data Parsing
// ============================================================================

describe('Event Data Parsing', () => {
  describe('parseStreamEvent', () => {
    it('parses valid JSON string to typed object', () => {
      const json = '{"job_id":"job-123","progress_pct":50,"status":"running"}';
      const result = parseStreamEvent<TrainingProgressEvent>(json);
      expect(result.job_id).toBe('job-123');
      expect(result.progress_pct).toBe(50);
      expect(result.status).toBe('running');
    });

    it('parses nested objects correctly', () => {
      const json = JSON.stringify({
        adapter_id: 'adapter-1',
        adapter_name: 'test-adapter',
        previous_state: 'cold',
        current_state: 'warm',
        timestamp: Date.now(),
        activation_percentage: 75,
      });
      const result = parseStreamEvent<AdapterStateTransitionEvent>(json);
      expect(result.adapter_id).toBe('adapter-1');
      expect(result.current_state).toBe('warm');
      expect(result.activation_percentage).toBe(75);
    });

    it('parses arrays in events', () => {
      const json = JSON.stringify({
        event: 'Done',
        total_tokens: 100,
        latency_ms: 250,
        unavailable_pinned_adapters: ['adapter-1', 'adapter-2'],
      });
      type DoneEvent = Extract<InferenceEvent, { event: 'Done' }>;
      const result = parseStreamEvent<DoneEvent>(json);
      expect(result.event).toBe('Done');
      expect(result.unavailable_pinned_adapters).toEqual(['adapter-1', 'adapter-2']);
    });

    it('throws on invalid JSON', () => {
      const invalidJson = '{ invalid json }';
      expect(() => parseStreamEvent(invalidJson)).toThrow('Failed to parse stream event');
    });

    it('throws on malformed JSON with helpful error message', () => {
      const malformed = '{"unclosed": ';
      expect(() => parseStreamEvent(malformed)).toThrow(/Failed to parse stream event/);
    });

    it('handles empty object JSON', () => {
      const result = parseStreamEvent('{}');
      expect(result).toEqual({});
    });

    it('handles null values in JSON', () => {
      const json = '{"value": null}';
      const result = parseStreamEvent<{ value: null }>(json);
      expect(result.value).toBeNull();
    });

    it('handles numeric strings correctly', () => {
      const json = '{"count": 42, "ratio": 3.14}';
      const result = parseStreamEvent<{ count: number; ratio: number }>(json);
      expect(result.count).toBe(42);
      expect(result.ratio).toBe(3.14);
    });
  });

  describe('type guards for various stream events', () => {
    describe('isTrainingProgressEvent', () => {
      it('returns true for valid training progress event', () => {
        const event = {
          job_id: 'job-123',
          dataset_id: 'dataset-456',
          status: 'running',
          progress_pct: 50,
          timestamp: new Date().toISOString(),
        };
        expect(isTrainingProgressEvent(event)).toBe(true);
      });

      it('returns false when missing required fields', () => {
        expect(isTrainingProgressEvent({ job_id: 'job-123' })).toBe(false);
        expect(isTrainingProgressEvent({ progress_pct: 50 })).toBe(false);
        expect(isTrainingProgressEvent({})).toBe(false);
      });
    });

    describe('isAdapterStateTransitionEvent', () => {
      it('returns true for valid adapter state transition', () => {
        const event = {
          adapter_id: 'adapter-1',
          adapter_name: 'test',
          previous_state: 'cold',
          current_state: 'warm',
          timestamp: Date.now(),
          activation_percentage: 100,
        };
        expect(isAdapterStateTransitionEvent(event)).toBe(true);
      });

      it('returns false when missing adapter_id or current_state', () => {
        expect(isAdapterStateTransitionEvent({ adapter_id: 'id' })).toBe(false);
        expect(isAdapterStateTransitionEvent({ current_state: 'warm' })).toBe(false);
      });
    });

    describe('isSystemMetricsEvent', () => {
      it('returns true for valid system metrics event', () => {
        const event = {
          timestamp: new Date().toISOString(),
          cpu: { usage_percent: 50, cores: 8 },
          memory: { used_gb: 8, total_gb: 16, usage_percent: 50 },
          disk: { used_gb: 100, total_gb: 500, usage_percent: 20 },
          network: {},
        };
        expect(isSystemMetricsEvent(event)).toBe(true);
      });

      it('returns false when missing required nested fields', () => {
        expect(isSystemMetricsEvent({ cpu: {}, memory: {} })).toBe(false);
        expect(isSystemMetricsEvent({ cpu: {}, disk: {} })).toBe(false);
        expect(isSystemMetricsEvent({ memory: {}, disk: {} })).toBe(false);
      });
    });

    describe('isStreamingChunk', () => {
      it('returns true for valid OpenAI-compatible streaming chunk', () => {
        const chunk = {
          id: 'chunk-1',
          object: 'chat.completion.chunk',
          created: Date.now(),
          model: 'qwen-2.5',
          choices: [{ index: 0, delta: { content: 'Hello' }, finish_reason: null }],
        };
        expect(isStreamingChunk(chunk)).toBe(true);
      });

      it('returns false for non-chunk objects', () => {
        expect(isStreamingChunk({ object: 'chat.completion' })).toBe(false);
        expect(isStreamingChunk({ object: 'chat.completion.chunk' })).toBe(false);
        expect(isStreamingChunk({ choices: [] })).toBe(false);
      });
    });

    describe('isComplianceChangedEvent', () => {
      it('returns true for valid compliance changed event', () => {
        const event = {
          event_type: 'compliance_changed',
          stack_id: 'stack-1',
          previous_score: 80,
          current_score: 95,
          previous_status: 'warning',
          current_status: 'compliant',
          changed_categories: ['security'],
          timestamp: new Date().toISOString(),
        };
        expect(isComplianceChangedEvent(event)).toBe(true);
      });

      it('returns false for other event types', () => {
        expect(isComplianceChangedEvent({ event_type: 'violation_detected' })).toBe(false);
      });
    });

    describe('isViolationDetectedEvent', () => {
      it('returns true for valid violation detected event', () => {
        const event = {
          event_type: 'violation_detected',
          stack_id: 'stack-1',
          violation_id: 'viol-1',
          policy_pack_id: 'pack-1',
          policy_name: 'Test Policy',
          severity: 'high',
          message: 'Violation detected',
          resource_type: 'adapter',
          resource_id: 'adapter-1',
          timestamp: new Date().toISOString(),
        };
        expect(isViolationDetectedEvent(event)).toBe(true);
      });
    });

    describe('isViolationResolvedEvent', () => {
      it('returns true for valid violation resolved event', () => {
        const event = {
          event_type: 'violation_resolved',
          stack_id: 'stack-1',
          violation_id: 'viol-1',
          policy_pack_id: 'pack-1',
          policy_name: 'Test Policy',
          resolved_by: 'user-1',
          timestamp: new Date().toISOString(),
        };
        expect(isViolationResolvedEvent(event)).toBe(true);
      });
    });
  });

  describe('boot progress event type guards', () => {
    describe('isStateChangedEvent', () => {
      it('returns true for valid state changed event', () => {
        const event = {
          event_type: 'StateChanged',
          previous: 'initializing',
          current: 'loading',
          elapsed_ms: 1000,
          models_pending: 2,
          models_ready: 1,
        };
        expect(isStateChangedEvent(event)).toBe(true);
      });
    });

    describe('isDownloadProgressEvent', () => {
      it('returns true for valid download progress event', () => {
        const event = {
          event_type: 'DownloadProgress',
          model_id: 'model-1',
          repo_id: 'org/model',
          downloaded_bytes: 1000000,
          total_bytes: 5000000,
          speed_mbps: 50,
          eta_seconds: 80,
          files_completed: 2,
          files_total: 5,
        };
        expect(isDownloadProgressEvent(event)).toBe(true);
      });
    });

    describe('isLoadProgressEvent', () => {
      it('returns true for valid load progress event', () => {
        const event = {
          event_type: 'LoadProgress',
          model_id: 'model-1',
          phase: 'loading',
          progress_pct: 50,
          memory_allocated_mb: 2048,
        };
        expect(isLoadProgressEvent(event)).toBe(true);
      });
    });

    describe('isModelReadyEvent', () => {
      it('returns true for valid model ready event', () => {
        const event = {
          event_type: 'ModelReady',
          model_id: 'model-1',
          warmup_latency_ms: 150,
          memory_usage_mb: 4096,
        };
        expect(isModelReadyEvent(event)).toBe(true);
      });
    });

    describe('isFullyReadyEvent', () => {
      it('returns true for valid fully ready event', () => {
        const event = {
          event_type: 'FullyReady',
          total_models: 3,
          total_download_mb: 15000,
          total_load_time_ms: 45000,
        };
        expect(isFullyReadyEvent(event)).toBe(true);
      });
    });
  });

  describe('inference event type guards', () => {
    describe('isInferenceEvent', () => {
      it('returns true for all valid inference event types', () => {
        const events: InferenceEvent[] = [
          { event: 'Loading', phase: 'Downloading', progress: 50 },
          { event: 'Ready', warmup_latency_ms: 100 },
          { event: 'Token', text: 'Hello' },
          { event: 'Done', total_tokens: 50, latency_ms: 200 },
          { event: 'Error', message: 'Error occurred', recoverable: true },
        ];

        events.forEach((event) => {
          expect(isInferenceEvent(event)).toBe(true);
        });
      });

      it('returns false for invalid event types', () => {
        expect(isInferenceEvent({ event: 'Unknown' })).toBe(false);
        expect(isInferenceEvent({ type: 'Token' })).toBe(false);
        expect(isInferenceEvent(null)).toBe(false);
        expect(isInferenceEvent({})).toBe(false);
      });
    });

    describe('specific inference event type guards', () => {
      it('isInferenceLoadingEvent identifies Loading events', () => {
        expect(isInferenceLoadingEvent({ event: 'Loading', phase: 'Warmup', progress: 75 })).toBe(
          true
        );
        expect(isInferenceLoadingEvent({ event: 'Ready', warmup_latency_ms: 100 })).toBe(false);
      });

      it('isInferenceReadyEvent identifies Ready events', () => {
        expect(isInferenceReadyEvent({ event: 'Ready', warmup_latency_ms: 100 })).toBe(true);
        expect(isInferenceReadyEvent({ event: 'Token', text: 'hi' })).toBe(false);
      });

      it('isInferenceTokenEvent identifies Token events', () => {
        expect(isInferenceTokenEvent({ event: 'Token', text: 'world' })).toBe(true);
        expect(isInferenceTokenEvent({ event: 'Done', total_tokens: 10, latency_ms: 50 })).toBe(
          false
        );
      });

      it('isInferenceDoneEvent identifies Done events', () => {
        expect(isInferenceDoneEvent({ event: 'Done', total_tokens: 100, latency_ms: 500 })).toBe(
          true
        );
        expect(isInferenceDoneEvent({ event: 'Error', message: 'err', recoverable: false })).toBe(
          false
        );
      });

      it('isInferenceErrorEvent identifies Error events', () => {
        expect(isInferenceErrorEvent({ event: 'Error', message: 'Failed', recoverable: true })).toBe(
          true
        );
        expect(isInferenceErrorEvent({ event: 'Loading', phase: 'Downloading', progress: 0 })).toBe(
          false
        );
      });
    });
  });

  describe('session progress event type guards', () => {
    describe('isSessionProgressEvent', () => {
      it('returns true for valid session progress event', () => {
        const event: SessionProgressEvent = {
          session_id: 'session-123',
          phase: 'parsing',
          percentage_complete: 45,
          message: 'Processing files...',
          timestamp: new Date().toISOString(),
        };
        expect(isSessionProgressEvent(event)).toBe(true);
      });

      it('returns true with optional fields', () => {
        const event: SessionProgressEvent = {
          session_id: 'session-123',
          dataset_id: 'dataset-456',
          phase: 'analyzing',
          sub_phase: 'tokenizing',
          current_file: 'data.json',
          percentage_complete: 60,
          phase_percentage: 30,
          total_files: 10,
          files_processed: 6,
          total_bytes: 1000000,
          bytes_processed: 600000,
          message: 'Analyzing files',
          timestamp: new Date().toISOString(),
          metadata: { custom: 'data' },
        };
        expect(isSessionProgressEvent(event)).toBe(true);
      });

      it('returns false when missing required fields', () => {
        expect(
          isSessionProgressEvent({
            phase: 'parsing',
            percentage_complete: 50,
            message: 'test',
            timestamp: new Date().toISOString(),
          })
        ).toBe(false);
      });
    });

    describe('isDatasetProgressEvent', () => {
      it('returns true for valid dataset progress event', () => {
        const event = {
          dataset_id: 'dataset-123',
          event_type: 'upload',
          percentage_complete: 75,
          message: 'Uploading files',
          timestamp: new Date().toISOString(),
        };
        expect(isDatasetProgressEvent(event)).toBe(true);
      });

      it('returns false when session_id is present (prefer SessionProgressEvent)', () => {
        const event = {
          session_id: 'session-1',
          dataset_id: 'dataset-123',
          event_type: 'upload',
          percentage_complete: 75,
          message: 'Uploading',
          timestamp: new Date().toISOString(),
        };
        expect(isDatasetProgressEvent(event)).toBe(false);
      });
    });

    describe('ingestion phase helpers', () => {
      it('isIngestionComplete returns true for completed and failed phases', () => {
        expect(isIngestionComplete('completed')).toBe(true);
        expect(isIngestionComplete('failed')).toBe(true);
      });

      it('isIngestionComplete returns false for in-progress phases', () => {
        const inProgressPhases: IngestionPhase[] = [
          'scanning',
          'parsing',
          'analyzing',
          'generating',
          'uploading',
          'validating',
          'computing_statistics',
        ];
        inProgressPhases.forEach((phase) => {
          expect(isIngestionComplete(phase)).toBe(false);
        });
      });

      it('isIngestionFailed returns true only for failed phase', () => {
        expect(isIngestionFailed('failed')).toBe(true);
        expect(isIngestionFailed('completed')).toBe(false);
        expect(isIngestionFailed('parsing')).toBe(false);
      });
    });
  });
});

// ============================================================================
// Type Safety for Reconnection Handling
// ============================================================================

describe('Type Safety for Reconnection Handling', () => {
  describe('StreamConfig type safety', () => {
    it('enforces correct callback signatures', () => {
      const config: StreamConfig = {
        enabled: true,
        onMessage: (data: unknown) => {
          // Should accept unknown data type
          expect(data).toBeDefined();
        },
        onError: (error: Event) => {
          // Should receive Event type
          expect(error).toBeDefined();
        },
        onOpen: () => {
          // No parameters
        },
        onClose: () => {
          // No parameters
        },
        autoReconnect: true,
        maxReconnectAttempts: 5,
        initialBackoffMs: 1000,
        maxBackoffMs: 30000,
      };

      // Type assertions
      expectTypeOf(config.enabled).toEqualTypeOf<boolean | undefined>();
      expectTypeOf(config.autoReconnect).toEqualTypeOf<boolean | undefined>();
      expectTypeOf(config.maxReconnectAttempts).toEqualTypeOf<number | undefined>();
      expectTypeOf(config.initialBackoffMs).toEqualTypeOf<number | undefined>();
      expectTypeOf(config.maxBackoffMs).toEqualTypeOf<number | undefined>();
    });

    it('allows partial configuration', () => {
      const minimalConfig: StreamConfig = {
        onMessage: () => {},
      };
      expect(minimalConfig.onMessage).toBeDefined();
      expect(minimalConfig.onError).toBeUndefined();
    });

    it('allows empty configuration', () => {
      const emptyConfig: StreamConfig = {};
      expect(emptyConfig).toBeDefined();
    });
  });

  describe('StreamState type safety', () => {
    it('enforces generic data type', () => {
      type TestData = { value: number };
      const state: StreamState<TestData> = {
        data: { value: 42 },
        error: null,
        connected: true,
        reconnect: () => {},
        lastUpdated: new Date().toISOString(),
      };

      expect(state.data?.value).toBe(42);
      expectTypeOf(state.data).toEqualTypeOf<TestData | null>();
      expectTypeOf(state.error).toEqualTypeOf<string | null>();
      expectTypeOf(state.connected).toEqualTypeOf<boolean>();
      expectTypeOf(state.reconnect).toEqualTypeOf<() => void>();
      expectTypeOf(state.lastUpdated).toEqualTypeOf<string | undefined>();
    });

    it('allows null data when disconnected', () => {
      const state: StreamState<TrainingProgressEvent> = {
        data: null,
        error: 'Connection lost',
        connected: false,
        reconnect: () => {},
      };

      expect(state.data).toBeNull();
      expect(state.error).toBe('Connection lost');
      expect(state.connected).toBe(false);
    });

    it('reconnect function is callable', () => {
      let reconnectCalled = false;
      const state: StreamState<unknown> = {
        data: null,
        error: null,
        connected: false,
        reconnect: () => {
          reconnectCalled = true;
        },
      };

      state.reconnect();
      expect(reconnectCalled).toBe(true);
    });
  });

  describe('reconnection scenario type safety', () => {
    it('handles reconnection with event gap correctly', () => {
      type ReconnectionResult = {
        success: boolean;
        gap?: SseEventGapDetectedEvent;
        action?: ReturnType<typeof getGapRecoveryAction>;
      };

      const handleReconnection = (event: SseErrorEvent): ReconnectionResult => {
        if (isSseEventGapDetectedEvent(event)) {
          return {
            success: false,
            gap: event,
            action: getGapRecoveryAction(event.recovery_hint),
          };
        }
        return { success: true };
      };

      const gapEvent: SseEventGapDetectedEvent = {
        type: 'event_gap',
        client_last_id: 100,
        server_oldest_id: 150,
        events_lost: 50,
        recovery_hint: { type: 'refetch_full_state' },
      };

      const result = handleReconnection(gapEvent);
      expect(result.success).toBe(false);
      expect(result.gap).toBeDefined();
      expect(result.action?.action).toBe('refetch');
      expect(result.action?.requiresRefresh).toBe(true);
    });

    it('handles heartbeat for connection health check', () => {
      const checkConnection = (event: SseErrorEvent): boolean => {
        if (isSseHeartbeatEvent(event)) {
          // Connection is alive, heartbeat received
          return true;
        }
        return false;
      };

      const heartbeat: SseHeartbeatEvent = {
        type: 'heartbeat',
        current_id: 500,
        timestamp_ms: Date.now(),
      };

      expect(checkConnection(heartbeat)).toBe(true);
    });

    it('handles stream disconnect with reconnection timing', () => {
      const getReconnectDelay = (event: SseErrorEvent): number | null => {
        if (isSseStreamDisconnectedEvent(event)) {
          return event.reconnect_hint_ms;
        }
        return null;
      };

      const disconnect: SseStreamDisconnectedEvent = {
        type: 'stream_disconnected',
        last_event_id: 200,
        reason: 'Server maintenance',
        reconnect_hint_ms: 10000,
      };

      expect(getReconnectDelay(disconnect)).toBe(10000);
    });

    it('handles buffer overflow with state recovery', () => {
      type RecoveryState = {
        needsFullRefresh: boolean;
        oldestAvailableId: number;
        droppedCount: number;
      };

      const handleOverflow = (event: SseErrorEvent): RecoveryState | null => {
        if (isSseBufferOverflowEvent(event)) {
          return {
            needsFullRefresh: event.dropped_count > 10,
            oldestAvailableId: event.oldest_available_id,
            droppedCount: event.dropped_count,
          };
        }
        return null;
      };

      const overflow: SseBufferOverflowEvent = {
        type: 'buffer_overflow',
        dropped_count: 25,
        oldest_available_id: 175,
      };

      const recovery = handleOverflow(overflow);
      expect(recovery).not.toBeNull();
      expect(recovery?.needsFullRefresh).toBe(true);
      expect(recovery?.oldestAvailableId).toBe(175);
      expect(recovery?.droppedCount).toBe(25);
    });
  });

  describe('exhaustive event handling', () => {
    it('handles all SSE error event types exhaustively', () => {
      const handleEvent = (event: SseErrorEvent): string => {
        if (isSseStreamDisconnectedEvent(event)) {
          return 'disconnected';
        }
        if (isSseBufferOverflowEvent(event)) {
          return 'overflow';
        }
        if (isSseEventGapDetectedEvent(event)) {
          return 'gap';
        }
        if (isSseHeartbeatEvent(event)) {
          return 'heartbeat';
        }
        // This should never be reached if types are correct
        const _exhaustiveCheck: never = event;
        return _exhaustiveCheck;
      };

      expect(
        handleEvent({
          type: 'stream_disconnected',
          last_event_id: 1,
          reason: 'test',
          reconnect_hint_ms: 1000,
        })
      ).toBe('disconnected');

      expect(
        handleEvent({
          type: 'buffer_overflow',
          dropped_count: 5,
          oldest_available_id: 10,
        })
      ).toBe('overflow');

      expect(
        handleEvent({
          type: 'event_gap',
          client_last_id: 5,
          server_oldest_id: 10,
          events_lost: 5,
          recovery_hint: { type: 'continue_with_gap' },
        })
      ).toBe('gap');

      expect(
        handleEvent({
          type: 'heartbeat',
          current_id: 100,
          timestamp_ms: Date.now(),
        })
      ).toBe('heartbeat');
    });
  });
});
