/**
 * Streaming Inference Hook
 *
 * Provides a React hook for streaming inference with token-by-token updates.
 * Uses POST-based SSE for sending inference configuration while streaming responses.
 *
 * Endpoint: POST /v1/infer/stream
 * Format: OpenAI-compatible chat completion chunks
 *
 * Usage:
 * ```tsx
 * const {
 *   text,
 *   tokens,
 *   isStreaming,
 *   error,
 *   start,
 *   stop,
 *   reset,
 *   latencyMs,
 *   tokensPerSecond,
 * } = useStreamingInference();
 *
 * // Start streaming
 * await start({
 *   prompt: 'Hello, world!',
 *   max_tokens: 100,
 *   temperature: 0.7,
 * });
 *
 * // Stop streaming early
 * stop();
 *
 * // Reset state for new inference
 * reset();
 * ```
 */

import { useState, useRef, useCallback } from 'react';
import apiClient from '../api/client';
import { logger, toError } from '../utils/logger';
import type { InferRequest } from '../api/types';

// ============================================================================
// Types
// ============================================================================

/**
 * Configuration for streaming inference request
 */
export interface StreamingInferenceConfig extends Omit<InferRequest, 'stream'> {
  /** Prompt to send to the model */
  prompt: string;
 /** Maximum tokens to generate */
  max_tokens?: number;
  /** Temperature for sampling (0.0 - 2.0) */
  temperature?: number;
  /** Top-p sampling parameter */
  top_p?: number;
  /** Top-k sampling parameter */
  top_k?: number;
  /** Random seed for deterministic generation */
  seed?: number;
  /** Backend selection (auto|mlx|coreml|metal) */
  backend?: InferRequest['backend'];
  /** Model identifier */
  model?: string;
  /** Adapter stack to use */
  adapter_stack?: string[];
  /** Specific adapters to use */
  adapters?: string[];
  /** Whether to require evidence spans */
  require_evidence?: boolean;
}

/**
 * Individual token event from the stream
 */
export interface StreamToken {
  /** Token text content */
  content: string;
  /** Timestamp when token was received */
  timestamp: number;
  /** Token index in the sequence */
  index: number;
}

/**
 * Streaming inference hook result
 */
export interface UseStreamingInferenceResult {
  /** Accumulated response text */
  text: string;
  /** Array of individual tokens received */
  tokens: StreamToken[];
  /** Whether streaming is currently active */
  isStreaming: boolean;
  /** Error message if streaming failed */
  error: string | null;
  /** Start streaming inference */
  start: (config: StreamingInferenceConfig) => Promise<void>;
  /** Stop streaming (cancel) */
  stop: () => void;
  /** Reset state for new inference */
  reset: () => void;
  /** Total latency in milliseconds */
  latencyMs: number;
  /** Tokens generated per second */
  tokensPerSecond: number;
  /** Finish reason if completed */
  finishReason: 'stop' | 'length' | 'error' | null;
  /** Response ID from the stream */
  responseId: string | null;
}

/**
 * OpenAI-compatible chat completion chunk
 * data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hi"},...}]}
 */
interface ChatCompletionChunk {
  id: string;
  object: 'chat.completion.chunk';
  created?: number;
  model?: string;
  choices: Array<{
    index: number;
    delta: {
      content?: string;
      role?: string;
    };
    finish_reason: 'stop' | 'length' | null;
  }>;
  usage?: {
    prompt_tokens?: number;
    completion_tokens?: number;
    total_tokens?: number;
  };
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * React hook for streaming inference with token-by-token updates
 *
 * Handles:
 * - POST request with SSE response
 * - Token accumulation
 * - [DONE] event handling
 * - Error handling and recovery
 * - Cancellation support
 * - Performance metrics
 */
export function useStreamingInference(): UseStreamingInferenceResult {
  // State
  const [text, setText] = useState<string>('');
  const [tokens, setTokens] = useState<StreamToken[]>([]);
  const [isStreaming, setIsStreaming] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [latencyMs, setLatencyMs] = useState<number>(0);
  const [tokensPerSecond, setTokensPerSecond] = useState<number>(0);
  const [finishReason, setFinishReason] = useState<'stop' | 'length' | 'error' | null>(null);
  const [responseId, setResponseId] = useState<string | null>(null);

  // Refs for cleanup and timing
  const abortControllerRef = useRef<AbortController | null>(null);
  const startTimeRef = useRef<number>(0);
  const tokenCountRef = useRef<number>(0);

  /**
   * Reset all state for a new inference
   */
  const reset = useCallback(() => {
    setText('');
    setTokens([]);
    setIsStreaming(false);
    setError(null);
    setLatencyMs(0);
    setTokensPerSecond(0);
    setFinishReason(null);
    setResponseId(null);
    tokenCountRef.current = 0;
    startTimeRef.current = 0;

    // Cancel any ongoing stream
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  }, []);

  /**
   * Stop the current streaming inference
   */
  const stop = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    setIsStreaming(false);

    // Calculate final metrics
    const elapsed = Date.now() - startTimeRef.current;
    setLatencyMs(elapsed);
    if (elapsed > 0 && tokenCountRef.current > 0) {
      setTokensPerSecond((tokenCountRef.current / elapsed) * 1000);
    }

    logger.info('Streaming inference stopped', {
      component: 'useStreamingInference',
      operation: 'stop',
      tokenCount: tokenCountRef.current,
      elapsedMs: elapsed,
    });
  }, []);

  /**
   * Start streaming inference with the given configuration
   */
  const start = useCallback(async (config: StreamingInferenceConfig) => {
    // Reset state for new inference
    reset();

    // Set up abort controller
    abortControllerRef.current = new AbortController();
    const signal = abortControllerRef.current.signal;

    // Start timing
    startTimeRef.current = Date.now();
    setIsStreaming(true);

    logger.info('Starting streaming inference', {
      component: 'useStreamingInference',
      operation: 'start',
      maxTokens: config.max_tokens,
      temperature: config.temperature,
      hasAdapters: !!config.adapters?.length || !!config.adapter_stack?.length,
    });

    try {
      // Get auth token and base URL
      const token = apiClient.getToken();
      const baseUrl = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';
      const url = `${baseUrl}/v1/infer/stream`;

      // Build request body
      const requestBody: InferRequest = {
        ...config,
        stream: true,
      };

      // Make POST request with SSE response
      const response = await fetch(url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Accept': 'text/event-stream',
          ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
        },
        body: JSON.stringify(requestBody),
        signal,
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`HTTP ${response.status}: ${errorText}`);
      }

      if (!response.body) {
        throw new Error('Response body is null - streaming not supported');
      }

      // Process the SSE stream
      const reader = response.body.getReader();
      const decoder = new TextDecoder('utf-8');
      let buffer = '';
      let accumulatedText = '';

      while (true) {
        const { done, value } = await reader.read();

        if (done) {
          break;
        }

        // Decode chunk and add to buffer
        buffer += decoder.decode(value, { stream: true });

        // Process complete SSE events (separated by double newlines)
        const events = buffer.split('\n\n');
        buffer = events.pop() || ''; // Keep incomplete event in buffer

        for (const event of events) {
          if (!event.trim()) continue;

          // Parse SSE event
          const lines = event.split('\n');
          let eventData = '';

          for (const line of lines) {
            if (line.startsWith('data: ')) {
              eventData = line.slice(6);
            }
          }

          if (!eventData) continue;

          // Check for [DONE] signal
          if (eventData.trim() === '[DONE]') {
            logger.info('Streaming inference completed', {
              component: 'useStreamingInference',
              operation: 'done',
              tokenCount: tokenCountRef.current,
              textLength: accumulatedText.length,
            });
            setFinishReason('stop');
            break;
          }

          // Parse JSON chunk
          try {
            const chunk: ChatCompletionChunk = JSON.parse(eventData);

            // Set response ID from first chunk
            if (chunk.id && !responseId) {
              setResponseId(chunk.id);
            }

            // Process each choice
            for (const choice of chunk.choices) {
              const content = choice.delta?.content;

              if (content) {
                // Accumulate text
                accumulatedText += content;
                setText(accumulatedText);

                // Track token
                const tokenIndex = tokenCountRef.current++;
                const newToken: StreamToken = {
                  content,
                  timestamp: Date.now(),
                  index: tokenIndex,
                };
                setTokens(prev => [...prev, newToken]);

                // Update metrics periodically
                const elapsed = Date.now() - startTimeRef.current;
                if (elapsed > 0) {
                  setLatencyMs(elapsed);
                  setTokensPerSecond((tokenCountRef.current / elapsed) * 1000);
                }
              }

              // Check finish reason
              if (choice.finish_reason) {
                setFinishReason(choice.finish_reason);
              }
            }
          } catch (parseError) {
            logger.warn('Failed to parse SSE chunk', {
              component: 'useStreamingInference',
              operation: 'parse',
              data: eventData.substring(0, 100),
            });
          }
        }
      }

      // Final metrics update
      const totalElapsed = Date.now() - startTimeRef.current;
      setLatencyMs(totalElapsed);
      if (totalElapsed > 0 && tokenCountRef.current > 0) {
        setTokensPerSecond((tokenCountRef.current / totalElapsed) * 1000);
      }

      logger.info('Streaming inference finished', {
        component: 'useStreamingInference',
        operation: 'finish',
        tokenCount: tokenCountRef.current,
        totalMs: totalElapsed,
        tokensPerSec: totalElapsed > 0 ? (tokenCountRef.current / totalElapsed) * 1000 : 0,
      });

    } catch (err) {
      // Handle abort (user cancellation)
      if (err instanceof Error && err.name === 'AbortError') {
        logger.info('Streaming inference aborted by user', {
          component: 'useStreamingInference',
          operation: 'abort',
        });
        return;
      }

      // Handle other errors
      const errorMessage = err instanceof Error ? err.message : 'Unknown streaming error';
      setError(errorMessage);
      setFinishReason('error');

      logger.error('Streaming inference failed', {
        component: 'useStreamingInference',
        operation: 'error',
      }, toError(err));

    } finally {
      setIsStreaming(false);
      abortControllerRef.current = null;
    }
  }, [reset, responseId]);

  return {
    text,
    tokens,
    isStreaming,
    error,
    start,
    stop,
    reset,
    latencyMs,
    tokensPerSecond,
    finishReason,
    responseId,
  };
}

// ============================================================================
// Exports
// ============================================================================

export default useStreamingInference;
