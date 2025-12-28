import { useState, useRef, useCallback } from 'react';
import { toast } from 'sonner';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import { useTenant } from '@/providers/FeatureProviders';
import { useOperationLockOptional } from '@/contexts/OperationLockContext';
import type { ChatMessage, ThroughputStats } from '@/components/chat/ChatMessage';
import type { StreamingInferRequest } from '@/api/streaming-types';
import type { RunMetadataPayload, UseChatStreamingOptions, UseChatStreamingReturn } from '@/types/hooks';

type StreamTokenChunk = {
  token: string;
  content: string;
  timestamp: number;
  index: number;
  logprob?: number | null;
  routerScore?: number | null;
};

const pickScalar = (source: Record<string, unknown>, keys: string[]): string | number | boolean | undefined => {
  for (const key of keys) {
    if (!(key in source)) continue;
    const value = source[key];
    if (value === undefined || value === null) continue;
    if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
      return value;
    }
  }
  return undefined;
};

const toStringValue = (value: string | number | boolean | undefined): string | undefined => {
  if (value === undefined) return undefined;
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return undefined;
};

const asRecord = (value: unknown): Record<string, unknown> | null => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
};

const parseJsonRecord = (value: unknown): Record<string, unknown> | null => {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  if (!trimmed.startsWith('{')) return null;
  try {
    return asRecord(JSON.parse(trimmed));
  } catch {
    return null;
  }
};

const unwrapRunEnvelope = (raw: Record<string, unknown>): Record<string, unknown> | null => {
  const direct = asRecord(raw.run_envelope) ?? asRecord(raw.runEnvelope);
  if (direct) return direct;

  const eventName =
    typeof raw.event === 'string'
      ? raw.event
      : typeof raw.event_type === 'string'
        ? raw.event_type
        : typeof raw.type === 'string'
          ? raw.type
          : null;

  if (eventName === 'aos.run_envelope') {
    return asRecord(raw.data) ?? parseJsonRecord(raw.data);
  }

  return null;
};

const isRunEnvelopeEvent = (payload: unknown): boolean => {
  const record = asRecord(payload);
  if (!record) return false;
  if ('run_envelope' in record || 'runEnvelope' in record) return true;
  const eventName =
    typeof record.event === 'string'
      ? record.event
      : typeof record.event_type === 'string'
        ? record.event_type
        : typeof record.type === 'string'
          ? record.type
          : null;
  return eventName === 'aos.run_envelope';
};

const extractRunMetadata = (payload: unknown): RunMetadataPayload | null => {
  const raw = asRecord(payload) ?? parseJsonRecord(payload);
  if (!raw) return null;
  const envelope = unwrapRunEnvelope(raw);
  const nested = asRecord(raw.metadata);
  const nestedEnvelope = nested ? unwrapRunEnvelope(nested) : null;
  const sources = [envelope, nestedEnvelope, raw, nested].filter(Boolean) as Array<Record<string, unknown>>;

  const lookupScalar = (keys: string[]) => {
    for (const source of sources) {
      const value = pickScalar(source, keys);
      if (value !== undefined) return value;
    }
    return undefined;
  };
  const lookup = (keys: string[]) => toStringValue(lookupScalar(keys));

  const runId = lookup(['run_id', 'runId']);
  const requestId = lookup(['request_id', 'requestId', 'id']);
  const traceId = lookup(['trace_id', 'traceId']);
  const workspaceId = lookup(['workspace_id', 'workspaceId', 'tenant_id', 'tenantId']);
  const manifestHashB3 = lookup(['manifest_hash_b3', 'manifestHashB3']);
  const policyMaskDigestB3 = lookup(['policy_mask_digest_b3', 'policyMaskDigestB3', 'policy_mask_digest', 'policyMaskDigest']);
  const planId = lookup(['plan_id', 'planId']);
  const routerSeed = lookup(['router_seed', 'routerSeed']);
  const tickRaw = lookupScalar(['tick']);
  const tick = typeof tickRaw === 'number' || typeof tickRaw === 'string' ? tickRaw : undefined;
  const workerId = lookup(['worker_id', 'workerId']);
  const reasoningMode = lookup(['reasoning_mode', 'reasoningMode']);
  const determinismVersion = lookup(['determinism_version', 'determinismVersion']);
  const bootTraceId = lookup(['boot_trace_id', 'bootTraceId']);
  const createdAt = lookup(['created_at', 'createdAt']);
  const rawSeed = lookupScalar(['seed_material', 'seedMaterial', 'seed']);
  const seedMaterial = typeof rawSeed === 'boolean' ? String(rawSeed) : rawSeed;
  const seededFlag = lookupScalar(['seeded_via_hkdf', 'seededViaHkdf', 'hkdf_seeded']);
  const seededViaHkdf =
    seededFlag === undefined
      ? seedMaterial !== undefined
      : Boolean(seededFlag === true || seededFlag === 'true' || seededFlag === '1');

  if (
    !runId &&
    !requestId &&
    !traceId &&
    !workspaceId &&
    !manifestHashB3 &&
    !policyMaskDigestB3 &&
    !planId &&
    !routerSeed &&
    tick === undefined &&
    !workerId &&
    !reasoningMode &&
    !determinismVersion &&
    !bootTraceId &&
    !createdAt &&
    seedMaterial === undefined
  ) {
    return null;
  }

  const metadata: RunMetadataPayload = {
    runId,
    requestId,
    traceId,
    manifestHashB3,
    policyMaskDigestB3,
    planId,
    workerId,
    reasoningMode,
    seedMaterial,
    seededViaHkdf,
  };

  return Object.assign(metadata, {
    workspaceId,
    routerSeed,
    tick,
    determinismVersion,
    bootTraceId,
    createdAt,
  });
};

/**
 * Hook for managing chat message streaming with SSE.
 *
 * Handles:
 * - Message validation and sanitization
 * - SSE-based streaming via apiClient
 * - AbortController for cancellation
 * - Token counting and timing metrics
 * - Error handling with toast notifications
 *
 * @example
 * ```tsx
 * function ChatComponent() {
 *   const {
 *     isStreaming,
 *     streamedText,
 *     sendMessage,
 *     cancelStream,
 *     tokensReceived
 *   } = useChatStreaming({
 *     sessionId: currentSessionId,
 *     collectionId: selectedCollectionId,
 *     onMessageSent: (msg) => console.log('Sent:', msg),
 *     onStreamComplete: (response) => console.log('Complete:', response),
 *     onError: (err) => console.error('Error:', err)
 *   });
 *
 *   const handleSend = async () => {
 *     await sendMessage(input, ['adapter-1', 'adapter-2']);
 *   };
 *
 *   return (
 *     <div>
 *       {isStreaming && <div>Streaming: {streamedText}</div>}
 *       <button onClick={handleSend} disabled={isStreaming}>Send</button>
 *       <button onClick={cancelStream}>Cancel</button>
 *       <div>Tokens: {tokensReceived}</div>
 *     </div>
 *   );
 * }
 * ```
 */
export function useChatStreaming(options: UseChatStreamingOptions): UseChatStreamingReturn {
  const {
    sessionId,
    collectionId,
    documentId,
    onMessageSent,
    onStreamComplete,
    onError,
    onRunMetadata,
  } = options;

  // Tenant and operation lock for workspace isolation
  const { selectedTenant } = useTenant();
  const operationLock = useOperationLockOptional();
  const streamingLockIdRef = useRef<string | null>(null);

  // State
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamedText, setStreamedText] = useState('');
  const [currentRequestId, setCurrentRequestId] = useState<string | null>(null);
  const [tokensReceived, setTokensReceived] = useState(0);
  const [streamDuration, setStreamDuration] = useState<number | null>(null);
  const [chunks, setChunks] = useState<StreamTokenChunk[]>([]);

  // Refs for cancellation and timing
  const abortControllerRef = useRef<AbortController | null>(null);
  const streamStartTimeRef = useRef<number | null>(null);
  const tokenMetaRef = useRef<StreamTokenChunk[]>([]);

  // Refs for envelope-based token buffering
  const envelopeReceivedRef = useRef<boolean>(false);
  const pendingTokensRef = useRef<Array<{ token: string; chunk: unknown }>>([]);
  const envelopeRunIdRef = useRef<string | null>(null);
  const currentTraceIdRef = useRef<string | null>(null);
  const pendingTokensOverflowRef = useRef<boolean>(false);
  const pendingTokensFirstTimestampRef = useRef<number | null>(null);
  const envelopeTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Refs for chunk de-duplication
  const seenChunkIdsRef = useRef<Set<string>>(new Set());
  const lastSequenceRef = useRef<number>(-1);

  // Constants for buffer limits
  const PENDING_TOKENS_LIMIT = 1000;
  const ENVELOPE_TIMEOUT_MS = 5000;
  const SEEN_CHUNK_IDS_MAX = 10000;
  const SEEN_CHUNK_IDS_KEEP = 5000;

  const emitRunMetadata = useCallback(
    (payload: unknown, requestHint?: string, traceHint?: string): RunMetadataPayload | null => {
      const metadata = extractRunMetadata(payload);
      if (!metadata) return null;

      const resolvedRequestId =
        metadata.requestId ?? metadata.runId ?? requestHint ?? currentRequestId ?? undefined;
      const resolvedTraceId =
        metadata.traceId ?? metadata.runId ?? traceHint ?? metadata.requestId ?? currentRequestId ?? undefined;

      if (resolvedRequestId) {
        setCurrentRequestId((prev) => (prev === resolvedRequestId ? prev : resolvedRequestId));
      }

      if (onRunMetadata) {
        onRunMetadata({
          ...metadata,
          requestId: resolvedRequestId,
          traceId: resolvedTraceId,
        });
      }

      return {
        ...metadata,
        requestId: resolvedRequestId,
        traceId: resolvedTraceId,
      };
    },
    [currentRequestId, onRunMetadata]
  );

  /**
   * Reset streaming state to initial values
   */
  const resetStream = useCallback(() => {
    setStreamedText('');
    setCurrentRequestId(null);
    setTokensReceived(0);
    setStreamDuration(null);
    setChunks([]);
    tokenMetaRef.current = [];
    streamStartTimeRef.current = null;
    envelopeReceivedRef.current = false;
    pendingTokensRef.current = [];
    envelopeRunIdRef.current = null;
    currentTraceIdRef.current = null;
    pendingTokensOverflowRef.current = false;
    pendingTokensFirstTimestampRef.current = null;
    // Clear envelope timeout to prevent memory leaks
    if (envelopeTimeoutRef.current) {
      clearTimeout(envelopeTimeoutRef.current);
      envelopeTimeoutRef.current = null;
    }
    // Clear de-dup set to prevent memory leaks across sessions
    seenChunkIdsRef.current = new Set();
    lastSequenceRef.current = -1;
  }, []);

  /**
   * Release the streaming operation lock
   */
  const releaseStreamingLock = useCallback(() => {
    if (streamingLockIdRef.current && operationLock) {
      operationLock.releaseLock(streamingLockIdRef.current);
      streamingLockIdRef.current = null;
    }
  }, [operationLock]);

  /**
   * Cancel the current streaming request
   */
  const cancelStream = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
      setIsStreaming(false);
      releaseStreamingLock();

      // Calculate duration if streaming was in progress
      if (streamStartTimeRef.current) {
        setStreamDuration(Date.now() - streamStartTimeRef.current);
      }

      tokenMetaRef.current = [];
      setChunks([]);
      setStreamedText('');
      setTokensReceived(0);
      // Note: Do NOT reset seenChunkIdsRef here - only reset in resetStream
      // This preserves de-duplication state across cancel/retry cycles
      envelopeReceivedRef.current = false;
      pendingTokensRef.current = [];
      pendingTokensOverflowRef.current = false;
      pendingTokensFirstTimestampRef.current = null;
      // Clear envelope timeout to prevent memory leaks
      if (envelopeTimeoutRef.current) {
        clearTimeout(envelopeTimeoutRef.current);
        envelopeTimeoutRef.current = null;
      }

      logger.info('Stream cancelled by user', {
        component: 'useChatStreaming',
        requestId: currentRequestId ?? undefined,
        tokensReceived,
      });
    }
  }, [currentRequestId, tokensReceived, releaseStreamingLock]);

  /**
   * Validate and sanitize message content
   */
  const validateMessage = useCallback((content: string): string | null => {
    const trimmed = content.trim();

    if (!trimmed) {
      toast.error('Message cannot be empty');
      return null;
    }

    if (trimmed.length > 10000) {
      toast.error('Message is too long (max 10,000 characters)');
      return null;
    }

    return trimmed;
  }, []);

  /**
   * Send a message and stream the response
   */
  const sendMessage = useCallback(async (
    content: string,
    adapterIds: string[]
  ): Promise<void> => {
    // Validate message
    const validatedContent = validateMessage(content);
    if (!validatedContent) {
      return;
    }

    // Check if already streaming
    if (isStreaming) {
      toast.warning('A message is already being processed');
      return;
    }

    // Reset state for new stream
    resetStream();
    setIsStreaming(true);
    streamStartTimeRef.current = Date.now();

    // Create user message
    const userMessage: ChatMessage = {
      id: `user-${Date.now()}`,
      role: 'user',
      content: validatedContent,
      timestamp: new Date(),
    };

    // Notify message sent
    onMessageSent?.(userMessage);

    // Acquire operation lock to prevent workspace switching during streaming
    if (operationLock && selectedTenant) {
      streamingLockIdRef.current = operationLock.acquireLock(
        'streaming',
        'Chat inference in progress',
        selectedTenant
      );
    }

    // Create abort controller for cancellation
    abortControllerRef.current = new AbortController();
    let traceId: string | undefined = undefined;

    // Prepare request
    // Note: documentId is accepted in options but not yet supported by StreamingInferRequest API
    // It's stored here for future API support. Currently, collectionId provides document scoping.
    const request: StreamingInferRequest = {
      prompt: validatedContent,
      max_tokens: 500,
      temperature: 0.7,
      adapter_stack: adapterIds ?? [],
      ...(collectionId && { collection_id: collectionId }),
      ...(documentId && { document_id: documentId }),
      ...(sessionId && { session_id: sessionId }),
      ...(options.routingDeterminismMode && { routing_determinism_mode: options.routingDeterminismMode }),
      ...(options.adapterStrengthOverrides && { adapter_strength_overrides: options.adapterStrengthOverrides }),
    };

    try {
      let fullText = '';
      let tokenCount = 0;

      // Helper to process a single token (used for both buffered and direct tokens)
      const processToken = (token: string, chunk: unknown) => {
        if (token.length === 0) {
          return;
        }

        // Chunk de-duplication
        const chunkRecord = asRecord(chunk);
        const chunkId = chunkRecord
          ? toStringValue(pickScalar(chunkRecord, ['chunk_id', 'chunkId', 'id']))
          : undefined;
        const sequence = chunkRecord
          ? pickScalar(chunkRecord, ['sequence', 'seq', 'index'])
          : undefined;
        const seqNum = typeof sequence === 'number' ? sequence : undefined;

        if (chunkId && seenChunkIdsRef.current.has(chunkId)) {
          return; // Skip duplicate chunk
        }
        if (chunkId) {
          // Sliding window cleanup: if set exceeds max size, keep only the most recent entries
          if (seenChunkIdsRef.current.size > SEEN_CHUNK_IDS_MAX) {
            const arr = Array.from(seenChunkIdsRef.current);
            seenChunkIdsRef.current = new Set(arr.slice(-SEEN_CHUNK_IDS_KEEP));
          }
          seenChunkIdsRef.current.add(chunkId);
        }
        if (seqNum !== undefined) {
          if (seqNum <= lastSequenceRef.current) {
            return; // Skip out-of-order or duplicate sequence
          }
          lastSequenceRef.current = seqNum;
        }

        tokenCount++;
        fullText += token;
        setStreamedText(fullText);
        setTokensReceived(tokenCount);
        const choice = (chunk as unknown as { choices?: Array<Record<string, unknown>> })?.choices?.[0];
        let logprob: number | null = null;
        if (choice && typeof choice === 'object' && 'logprobs' in choice) {
          const logprobs = (choice as Record<string, unknown>).logprobs as {
            token_logprobs?: Array<number | null>;
            top_logprobs?: Array<Record<string, number>>;
          } | undefined;
          if (logprobs?.token_logprobs && logprobs.token_logprobs.length > 0) {
            const value = logprobs.token_logprobs[0];
            logprob = typeof value === 'number' ? value : value ?? null;
          } else if (logprobs?.top_logprobs && logprobs.top_logprobs.length > 0) {
            const first = logprobs.top_logprobs[0];
            const value = first && typeof first === 'object' ? Object.values(first)[0] : null;
            logprob = typeof value === 'number' ? value : value ?? null;
          }
        }
        const routerScore =
          (chunk as unknown as { router_score?: number })?.router_score ??
          (chunk as unknown as { metadata?: { router_score?: number } })?.metadata?.router_score ??
          null;
        const chunkEntry: StreamTokenChunk = {
          token,
          content: token,
          timestamp: Date.now(),
          index: tokenCount - 1,
          logprob,
          routerScore,
        };
        setChunks(prev => [
          ...prev,
          chunkEntry,
        ]);
        tokenMetaRef.current = [...tokenMetaRef.current.slice(-199), chunkEntry];
      };

      await apiClient.streamInfer(
        request,
        {
          onToken: (token: string, chunk) => {
            if (!traceId && typeof chunk.id === 'string') {
              traceId = chunk.id;
              currentTraceIdRef.current = chunk.id;
              setCurrentRequestId(chunk.id);
            }
            const runMetadata = emitRunMetadata(
              chunk,
              typeof (chunk as { id?: string }).id === 'string' ? (chunk as { id: string }).id : undefined,
              traceId ?? undefined
            );
            if (!traceId && runMetadata?.traceId) {
              traceId = runMetadata.traceId;
              currentTraceIdRef.current = runMetadata.traceId;
            } else if (!traceId && runMetadata?.requestId) {
              traceId = runMetadata.requestId;
              currentTraceIdRef.current = runMetadata.requestId;
            }

            // Check if this is an envelope event
            if (isRunEnvelopeEvent(chunk)) {
              // Mark envelope as received and store runId
              envelopeReceivedRef.current = true;
              if (runMetadata?.runId) {
                envelopeRunIdRef.current = runMetadata.runId;
              }

              // Clear envelope timeout since envelope arrived
              if (envelopeTimeoutRef.current) {
                clearTimeout(envelopeTimeoutRef.current);
                envelopeTimeoutRef.current = null;
              }
              pendingTokensFirstTimestampRef.current = null;

              // Flush any pending tokens that arrived before the envelope
              if (pendingTokensRef.current.length > 0) {
                for (const pending of pendingTokensRef.current) {
                  processToken(pending.token, pending.chunk);
                }
                pendingTokensRef.current = [];
              }
              pendingTokensOverflowRef.current = false;
              return;
            }

            // Skip empty tokens
            if (token.length === 0) {
              return;
            }

            // Buffer tokens until envelope is received (unless in overflow mode)
            if (!envelopeReceivedRef.current && !pendingTokensOverflowRef.current) {
              // Track first pending token timestamp for timeout
              if (pendingTokensFirstTimestampRef.current === null) {
                pendingTokensFirstTimestampRef.current = Date.now();
                // Set up envelope timeout
                envelopeTimeoutRef.current = setTimeout(() => {
                  if (!envelopeReceivedRef.current && pendingTokensRef.current.length > 0) {
                    logger.warn('Envelope timeout: flushing pending tokens without envelope context', {
                      component: 'useChatStreaming',
                      pendingCount: pendingTokensRef.current.length,
                      timeoutMs: ENVELOPE_TIMEOUT_MS,
                    });
                    // Process all pending tokens without envelope context
                    for (const pending of pendingTokensRef.current) {
                      processToken(pending.token, pending.chunk);
                    }
                    pendingTokensRef.current = [];
                    pendingTokensOverflowRef.current = true;
                  }
                  envelopeTimeoutRef.current = null;
                }, ENVELOPE_TIMEOUT_MS);
              }

              // Check pending tokens limit
              if (pendingTokensRef.current.length >= PENDING_TOKENS_LIMIT) {
                logger.warn('Pending tokens buffer exceeded limit, processing without envelope context', {
                  component: 'useChatStreaming',
                  limit: PENDING_TOKENS_LIMIT,
                  pendingCount: pendingTokensRef.current.length,
                });
                // Flush all pending tokens and switch to overflow mode
                for (const pending of pendingTokensRef.current) {
                  processToken(pending.token, pending.chunk);
                }
                pendingTokensRef.current = [];
                pendingTokensOverflowRef.current = true;
                // Clear the timeout since we're now in overflow mode
                if (envelopeTimeoutRef.current) {
                  clearTimeout(envelopeTimeoutRef.current);
                  envelopeTimeoutRef.current = null;
                }
                // Process the current token immediately
                processToken(token, chunk);
                return;
              }

              pendingTokensRef.current.push({ token, chunk });
              return;
            }

            // Process token normally after envelope received or in overflow mode
            processToken(token, chunk);
          },

          onComplete: (completedText, finishReason, metadata) => {
            // Clear envelope timeout to prevent memory leaks
            if (envelopeTimeoutRef.current) {
              clearTimeout(envelopeTimeoutRef.current);
              envelopeTimeoutRef.current = null;
            }

            // Calculate final duration
            const duration = streamStartTimeRef.current
              ? Date.now() - streamStartTimeRef.current
              : null;
            setStreamDuration(duration);

            const resolvedTraceId = metadata?.request_id || traceId;
            if (resolvedTraceId) {
              setCurrentRequestId(resolvedTraceId);
            }
            emitRunMetadata(metadata, resolvedTraceId ?? traceId ?? undefined, resolvedTraceId ?? traceId ?? undefined);

            // Calculate throughput stats from local variables (guaranteed accurate)
            const throughputStats: ThroughputStats | undefined =
              duration && duration > 0 && tokenCount > 0
                ? {
                    tokensGenerated: tokenCount,
                    latencyMs: duration,
                    tokensPerSecond: tokenCount / (duration / 1000),
                  }
                : undefined;

            const runMetadata = extractRunMetadata(metadata);

            // Create completed assistant message
            const assistantMessage: ChatMessage = {
              id: `assistant-${Date.now()}`,
              role: 'assistant',
              content: completedText,
              timestamp: new Date(),
              requestId: resolvedTraceId,
              traceId: resolvedTraceId,
              isStreaming: false,
              throughputStats,
              unavailablePinnedAdapters: metadata?.unavailable_pinned_adapters,
              pinnedRoutingFallback:
                metadata?.pinned_routing_fallback === 'stack_only' || metadata?.pinned_routing_fallback === 'partial'
                  ? metadata.pinned_routing_fallback
                  : undefined,
              tokenStream: tokenMetaRef.current,
              runMetadata: runMetadata
                ? {
                    ...runMetadata,
                    requestId: runMetadata.requestId ?? resolvedTraceId ?? traceId ?? undefined,
                    traceId: runMetadata.traceId ?? resolvedTraceId ?? traceId ?? undefined,
                  }
                : undefined,
            };

            logger.info('Stream completed', {
              component: 'useChatStreaming',
              traceId: resolvedTraceId,
              tokensReceived: tokenCount,
              duration,
              finishReason,
              sessionId: sessionId ?? undefined,
              unavailablePinnedAdapters: metadata?.unavailable_pinned_adapters,
              pinnedRoutingFallback: metadata?.pinned_routing_fallback,
            });

            setIsStreaming(false);
            releaseStreamingLock();

            // Notify completion
            onStreamComplete?.(assistantMessage);
          },

          onError: (error: Error) => {
            // Clear envelope timeout to prevent memory leaks
            if (envelopeTimeoutRef.current) {
              clearTimeout(envelopeTimeoutRef.current);
              envelopeTimeoutRef.current = null;
            }

            logger.error('Stream error', {
              component: 'useChatStreaming',
              sessionId: sessionId ?? undefined,
            }, error);

            // Emit accumulated metadata before clearing state (mid-stream disconnect preservation)
            if (envelopeRunIdRef.current || currentTraceIdRef.current) {
              emitRunMetadata({
                runId: envelopeRunIdRef.current ?? undefined,
                traceId: currentTraceIdRef.current,
                streamInterrupted: true,
                interruptReason: error.message,
              }, currentTraceIdRef.current ?? undefined, currentTraceIdRef.current ?? undefined);
            }

            // Calculate duration even on error
            if (streamStartTimeRef.current) {
              setStreamDuration(Date.now() - streamStartTimeRef.current);
            }

            setIsStreaming(false);
            releaseStreamingLock();
            setCurrentRequestId(null);

            toast.error(`Inference failed: ${error.message}`);

            // Notify error
            onError?.(error);
          },
        },
        abortControllerRef.current.signal
      );
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Inference failed');

      // Don't show error toast for user cancellations
      if (error.name !== 'AbortError') {
        toast.error(`Inference failed: ${error.message}`);
        logger.error('Stream request failed', {
          component: 'useChatStreaming',
          traceId,
          sessionId: sessionId ?? undefined,
        }, toError(err));

        onError?.(error);
      }

      // Calculate duration
      if (streamStartTimeRef.current) {
        setStreamDuration(Date.now() - streamStartTimeRef.current);
      }

      setIsStreaming(false);
      releaseStreamingLock();
      setCurrentRequestId(null);
    }
  }, [
    isStreaming,
    collectionId,
    documentId,
    sessionId,
    selectedTenant,
    operationLock,
    validateMessage,
    resetStream,
    releaseStreamingLock,
    onMessageSent,
    onStreamComplete,
    onError,
    emitRunMetadata,
  ]);

  return {
    // State
    isStreaming,
    streamedText,
    currentRequestId,
    chunks,

    // Actions
    sendMessage,
    cancelStream,
    resetStream,

    // Metrics
    tokensReceived,
    streamDuration,
  };
}
