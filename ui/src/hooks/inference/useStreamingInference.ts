/**
 * Streaming Inference Hook
 *
 * Manages streaming inference state and API integration for real-time token generation.
 * Extracted from InferencePlayground.tsx for reusability across components.
 *
 * Features:
 * - Real-time token streaming with Server-Sent Events (SSE)
 * - Token counting and throughput metrics (tokens per second)
 * - Graceful cancellation with AbortController
 * - Automatic cleanup on unmount
 * - Error handling with toast notifications
 * - Memory leak prevention with proper cleanup
 *
 * @example
 * ```tsx
 * const {
 *   streamingState,
 *   isStreaming,
 *   streamedText,
 *   tokensPerSecond,
 *   startStreaming,
 *   cancelStreaming,
 *   resetStreaming
 * } = useStreamingInference({
 *   config: {
 *     max_tokens: 100,
 *     temperature: 0.7,
 *     backend: 'auto',
 *   },
 *   adapterId: 'adapter-123',
 *   onToken: (token) => console.log('Token:', token),
 *   onComplete: (response) => console.log('Complete:', response),
 *   onError: (error) => console.error('Error:', error),
 * });
 *
 * // Start streaming
 * await startStreaming('Your prompt here');
 *
 * // Cancel mid-stream
 * cancelStreaming();
 *
 * // Reset state
 * resetStreaming();
 * ```
 */

import { useState, useRef, useCallback, useEffect } from 'react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import type { InferResponse, InferenceConfig, StreamingChunk, Citation } from '@/api/types';
import { logger, toError } from '@/utils/logger';

/** Individual streaming token with timestamp */
export interface StreamingToken {
  token: string;
  timestamp: number;
}

/** State for streaming inference operations */
export interface StreamingState {
  /** Whether a streaming operation is currently active */
  isStreaming: boolean;
  /** Accumulated text from all tokens */
  streamedText: string;
  /** Total number of tokens received */
  tokenCount: number;
  /** Timestamp when streaming started (ms) */
  startTime: number | null;
  /** Current throughput in tokens per second */
  tokensPerSecond: number;
}

/** Configuration options for the streaming inference hook */
export interface UseStreamingInferenceOptions {
  /** Inference configuration (max_tokens, temperature, etc.) */
  config: InferenceConfig;
  /** Optional adapter ID to use for inference */
  adapterId?: string;
  /** Optional stack ID to use for inference */
  stackId?: string;
  /** Callback invoked for each token received */
  onToken?: (token: string) => void;
  /** Callback invoked when streaming completes successfully */
  onComplete?: (response: InferResponse) => void;
  /** Callback invoked on error */
  onError?: (error: Error) => void;
}

/** Return value from useStreamingInference hook */
export interface UseStreamingInferenceReturn {
  /** Complete streaming state object */
  streamingState: StreamingState;
  /** Whether streaming is currently active */
  isStreaming: boolean;
  /** Accumulated streamed text */
  streamedText: string;
  /** Current tokens per second */
  tokensPerSecond: number;
  /** Start a new streaming inference operation */
  startStreaming: (prompt: string, overrides?: Partial<InferenceConfig>) => Promise<void>;
  /** Cancel the current streaming operation */
  cancelStreaming: () => void;
  /** Reset streaming state to initial values */
  resetStreaming: () => void;
}

/**
 * Hook for managing streaming inference operations.
 *
 * Handles real-time token streaming, metrics tracking, and lifecycle management
 * for SSE-based inference requests. Provides proper cleanup and error handling.
 */
export function useStreamingInference(
  options: UseStreamingInferenceOptions
): UseStreamingInferenceReturn {
  const { config, adapterId, stackId, onToken, onComplete, onError } = options;

  // Streaming state
  const [streamingState, setStreamingState] = useState<StreamingState>({
    isStreaming: false,
    streamedText: '',
    tokenCount: 0,
    startTime: null,
    tokensPerSecond: 0,
  });

  // Abort controller for cancellation
  const abortControllerRef = useRef<AbortController | null>(null);

  /**
   * Reset streaming state to initial values
   */
  const resetStreaming = useCallback(() => {
    setStreamingState({
      isStreaming: false,
      streamedText: '',
      tokenCount: 0,
      startTime: null,
      tokensPerSecond: 0,
    });
  }, []);

  /**
   * Cancel the current streaming operation
   */
  const cancelStreaming = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;

      logger.info('Streaming inference cancelled by user', {
        component: 'useStreamingInference',
        operation: 'cancelStreaming',
      });

      setStreamingState(prev => ({
        ...prev,
        isStreaming: false,
      }));
    }
  }, []);

  /**
   * Start a new streaming inference operation
   */
  const startStreaming = useCallback(async (prompt: string, overrides?: Partial<InferenceConfig>) => {
    // Reset state
    setStreamingState({
      isStreaming: true,
      streamedText: '',
      tokenCount: 0,
      startTime: Date.now(),
      tokensPerSecond: 0,
    });

    // Create new abort controller
    abortControllerRef.current = new AbortController();
    const startTime = Date.now();
    let tokenCount = 0;
    const effectiveBackend = overrides?.backend ?? config.backend ?? 'auto';
    const effectiveModel = overrides?.model ?? config.model;

    logger.info('Starting streaming inference', {
      component: 'useStreamingInference',
      operation: 'startStreaming',
      promptLength: prompt.length,
      adapterId,
      stackId,
      backend: effectiveBackend,
      model: effectiveModel,
    });

    try {
      await apiClient.streamInfer(
        {
          prompt,
          backend: effectiveBackend,
          model: effectiveModel,
          max_tokens: overrides?.max_tokens ?? config.max_tokens,
          temperature: overrides?.temperature ?? config.temperature,
          top_k: overrides?.top_k ?? config.top_k,
          top_p: overrides?.top_p ?? config.top_p,
          seed: overrides?.seed ?? config.seed,
          routing_determinism_mode: overrides?.routing_determinism_mode ?? config.routing_determinism_mode,
          adapter_stack: overrides?.adapter_stack
            ? overrides.adapter_stack as string[]
            : stackId
              ? [stackId]
              : (adapterId && adapterId !== 'none' ? [adapterId] : undefined),
          adapters: overrides?.adapters ?? undefined,
        },
        {
          onToken: (token: string, chunk: StreamingChunk) => {
            tokenCount++;
            const elapsed = (Date.now() - startTime) / 1000;
            const tokensPerSecond = elapsed > 0 ? tokenCount / elapsed : 0;

            setStreamingState(prev => ({
              ...prev,
              streamedText: prev.streamedText + token,
              tokenCount,
              tokensPerSecond,
            }));

            // Invoke optional callback
            if (onToken) {
              onToken(token);
            }
          },
          onComplete: (fullText: string, finishReason: string | null, metadata?: { unavailable_pinned_adapters?: string[], pinned_routing_fallback?: string, citations?: Citation[] }) => {
            const elapsed = Date.now() - startTime;

            // Map streaming finish reason to InferResponse finish reason
            const mapFinishReason = (reason: string | null): 'stop' | 'length' | 'error' => {
              if (reason === 'length') return 'length';
              if (reason === 'content_filter' || reason === 'error' || reason === 'cancelled') return 'error';
              return 'stop';
            };

            // Build final response (partial - streaming doesn't have all fields)
            const finalResponse: InferResponse = {
              schema_version: '1.0',
              id: `stream-${Date.now()}`,
              text: fullText,
              tokens_generated: tokenCount,
              token_count: tokenCount,
              latency_ms: elapsed,
              adapters_used: adapterId && adapterId !== 'none' ? [adapterId] : [],
              finish_reason: mapFinishReason(finishReason),
              citations: metadata?.citations || [],
              unavailable_pinned_adapters: metadata?.unavailable_pinned_adapters,
              pinned_routing_fallback: metadata?.pinned_routing_fallback as any,
            };

            setStreamingState(prev => ({
              ...prev,
              isStreaming: false,
            }));

            logger.info('Streaming inference completed', {
              component: 'useStreamingInference',
              operation: 'startStreaming',
              tokenCount,
              latencyMs: elapsed,
              finishReason,
            });

            // Invoke optional callback
            if (onComplete) {
              onComplete(finalResponse);
            }
          },
          onError: (error: Error) => {
            setStreamingState(prev => ({
              ...prev,
              isStreaming: false,
            }));

            logger.error('Streaming inference failed', {
              component: 'useStreamingInference',
              operation: 'startStreaming',
              adapterId,
              stackId,
            }, error);

            toast.error(`Streaming failed: ${error.message}`);

            // Invoke optional callback
            if (onError) {
              onError(error);
            }
          },
        },
        abortControllerRef.current.signal
      );
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Streaming inference failed');

      setStreamingState(prev => ({
        ...prev,
        isStreaming: false,
      }));

      logger.error('Streaming inference request failed', {
        component: 'useStreamingInference',
        operation: 'startStreaming',
        adapterId,
        stackId,
      }, toError(err));

      toast.error(`Streaming failed: ${error.message}`);

      // Invoke optional callback
      if (onError) {
        onError(error);
      }
    }
  }, [config, adapterId, stackId, onToken, onComplete, onError]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
        abortControllerRef.current = null;
      }
    };
  }, []);

  return {
    streamingState,
    isStreaming: streamingState.isStreaming,
    streamedText: streamingState.streamedText,
    tokensPerSecond: streamingState.tokensPerSecond,
    startStreaming,
    cancelStreaming,
    resetStreaming,
  };
}
