import { useState, useRef, useCallback } from 'react';
import { toast } from 'sonner';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import type { ChatMessage, ThroughputStats } from '@/components/chat/ChatMessage';
import type { StreamingInferRequest } from '@/api/streaming-types';
import type { UseChatStreamingOptions, UseChatStreamingReturn } from '@/types/hooks';

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
    onError
  } = options;

  // State
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamedText, setStreamedText] = useState('');
  const [currentRequestId, setCurrentRequestId] = useState<string | null>(null);
  const [tokensReceived, setTokensReceived] = useState(0);
  const [streamDuration, setStreamDuration] = useState<number | null>(null);
  const [chunks, setChunks] = useState<Array<{ content: string; timestamp: number; index: number }>>([]);

  // Refs for cancellation and timing
  const abortControllerRef = useRef<AbortController | null>(null);
  const streamStartTimeRef = useRef<number | null>(null);

  /**
   * Reset streaming state to initial values
   */
  const resetStream = useCallback(() => {
    setStreamedText('');
    setCurrentRequestId(null);
    setTokensReceived(0);
    setStreamDuration(null);
    setChunks([]);
    streamStartTimeRef.current = null;
  }, []);

  /**
   * Cancel the current streaming request
   */
  const cancelStream = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
      setIsStreaming(false);

      // Calculate duration if streaming was in progress
      if (streamStartTimeRef.current) {
        setStreamDuration(Date.now() - streamStartTimeRef.current);
      }

      logger.info('Stream cancelled by user', {
        component: 'useChatStreaming',
        requestId: currentRequestId ?? undefined,
        tokensReceived,
      });
    }
  }, [currentRequestId, tokensReceived]);

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

      await apiClient.streamInfer(
        request,
        {
          onToken: (token: string, chunk) => {
            if (!traceId && typeof chunk.id === 'string') {
              traceId = chunk.id;
              setCurrentRequestId(chunk.id);
            }
            tokenCount++;
            fullText += token;
            setStreamedText(fullText);
            setTokensReceived(tokenCount);
            setChunks(prev => [
              ...prev,
              {
                content: token,
                timestamp: Date.now(),
                index: tokenCount - 1,
              },
            ]);
          },

          onComplete: (completedText, finishReason, metadata) => {
            // Calculate final duration
            const duration = streamStartTimeRef.current
              ? Date.now() - streamStartTimeRef.current
              : null;
            setStreamDuration(duration);

            const resolvedTraceId = metadata?.request_id || traceId;
            if (resolvedTraceId) {
              setCurrentRequestId(resolvedTraceId);
            }

            // Calculate throughput stats from local variables (guaranteed accurate)
            const throughputStats: ThroughputStats | undefined =
              duration && duration > 0 && tokenCount > 0
                ? {
                    tokensGenerated: tokenCount,
                    latencyMs: duration,
                    tokensPerSecond: tokenCount / (duration / 1000),
                  }
                : undefined;

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

            // Notify completion
            onStreamComplete?.(assistantMessage);
          },

          onError: (error: Error) => {
            logger.error('Stream error', {
              component: 'useChatStreaming',
              sessionId: sessionId ?? undefined,
            }, error);

            // Calculate duration even on error
            if (streamStartTimeRef.current) {
              setStreamDuration(Date.now() - streamStartTimeRef.current);
            }

            setIsStreaming(false);
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
      setCurrentRequestId(null);
    }
  }, [
    isStreaming,
    collectionId,
    documentId,
    sessionId,
    validateMessage,
    resetStream,
    onMessageSent,
    onStreamComplete,
    onError,
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
