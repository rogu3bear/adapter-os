/**
 * Inference Streaming Hook
 *
 * Custom React hook for consuming the /v1/infer/stream SSE endpoint.
 * Handles OpenAI-compatible chat completion chunk format with token-by-token updates.
 *
 * Features:
 * - SSE connection management with reconnection
 * - Token accumulation and tracking
 * - [DONE] event detection
 * - Timing metrics (tokens/sec, latency)
 * - Error handling and recovery
 * - Stop sequences support
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
 *   tokensPerSecond,
 * } = useInferenceStream({
 *   prompt: 'Hello, world!',
 *   adapters: ['my-adapter'],
 *   maxTokens: 100,
 * });
 * ```
 */

import { useState, useRef, useCallback, useEffect } from 'react';
import { logger, toError } from '@/utils/logger';

// ============================================================================
// Types
// ============================================================================

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
 * Configuration options for the inference stream
 */
export interface InferenceStreamOptions {
  /** The prompt to send for inference */
  prompt: string;
  /** Model identifier (optional) */
  model?: string;
  /** Adapter stack to use */
  adapters?: string[];
  /** Adapter stack ID to use */
  stackId?: string;
  /** Maximum tokens to generate */
  maxTokens?: number;
  /** Sampling temperature (0.0 - 2.0) */
  temperature?: number;
  /** Top-p sampling parameter */
  topP?: number;
  /** Top-k sampling parameter */
  topK?: number;
  /** Stop sequences to terminate generation */
  stopSequences?: string[];
  /** Random seed for deterministic generation */
  seed?: number;
  /** Enable the stream (default: true) */
  enabled?: boolean;
  /** Callback when stream completes */
  onComplete?: (text: string) => void;
  /** Callback on error */
  onError?: (error: Error) => void;
  /** Callback on each token */
  onToken?: (token: StreamToken) => void;
}

/**
 * Hook result
 */
export interface InferenceStreamResult {
  /** Accumulated response text */
  text: string;
  /** Array of individual tokens received */
  tokens: StreamToken[];
  /** Whether streaming is currently active */
  isStreaming: boolean;
  /** Whether SSE connection is established */
  connected: boolean;
  /** Error message if streaming failed */
  error: Error | null;
  /** Start streaming inference */
  start: () => void;
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
export function useInferenceStream(
  options: InferenceStreamOptions
): InferenceStreamResult {
  const {
    prompt,
    model,
    adapters,
    stackId,
    maxTokens = 512,
    temperature = 0.7,
    topP,
    topK,
    stopSequences,
    seed,
    enabled = true,
    onComplete,
    onError,
    onToken,
  } = options;

  // State
  const [text, setText] = useState<string>('');
  const [tokens, setTokens] = useState<StreamToken[]>([]);
  const [isStreaming, setIsStreaming] = useState<boolean>(false);
  const [connected, setConnected] = useState<boolean>(false);
  const [error, setError] = useState<Error | null>(null);
  const [latencyMs, setLatencyMs] = useState<number>(0);
  const [tokensPerSecond, setTokensPerSecond] = useState<number>(0);
  const [finishReason, setFinishReason] = useState<'stop' | 'length' | 'error' | null>(null);
  const [responseId, setResponseId] = useState<string | null>(null);

  // Refs for cleanup and timing
  const abortControllerRef = useRef<AbortController | null>(null);
  const startTimeRef = useRef<number>(0);
  const tokenCountRef = useRef<number>(0);
  const isMountedRef = useRef<boolean>(true);

  // Store callbacks in refs to avoid dependency issues
  const onCompleteRef = useRef(onComplete);
  const onErrorRef = useRef(onError);
  const onTokenRef = useRef(onToken);

  useEffect(() => {
    onCompleteRef.current = onComplete;
    onErrorRef.current = onError;
    onTokenRef.current = onToken;
  }, [onComplete, onError, onToken]);

  /**
   * Reset all state for a new inference
   */
  const reset = useCallback(() => {
    setText('');
    setTokens([]);
    setIsStreaming(false);
    setConnected(false);
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
    setConnected(false);

    // Calculate final metrics
    const elapsed = Date.now() - startTimeRef.current;
    setLatencyMs(elapsed);
    if (elapsed > 0 && tokenCountRef.current > 0) {
      setTokensPerSecond((tokenCountRef.current / elapsed) * 1000);
    }

    logger.info('Streaming inference stopped', {
      component: 'useInferenceStream',
      operation: 'stop',
      tokenCount: tokenCountRef.current,
      elapsedMs: elapsed,
    });
  }, []);

  /**
   * Start streaming inference
   */
  const start = useCallback(async () => {
    if (!enabled || !prompt) {
      return;
    }

    // Reset state for new inference
    reset();

    // Set up abort controller
    abortControllerRef.current = new AbortController();
    const signal = abortControllerRef.current.signal;

    // Start timing
    startTimeRef.current = Date.now();
    setIsStreaming(true);
    setConnected(true);

    logger.info('Starting streaming inference', {
      component: 'useInferenceStream',
      operation: 'start',
      promptLength: prompt.length,
      maxTokens,
      temperature,
      hasAdapters: !!adapters?.length || !!stackId,
    });

    try {
      // Build URL for streaming inference
      const baseUrl = (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL || '/api';
      const url = `${baseUrl}/v1/infer/stream`;

      // Build request body
      const requestBody = {
        prompt,
        model,
        adapters,
        stack_id: stackId,
        max_tokens: maxTokens,
        temperature,
        top_p: topP,
        top_k: topK,
        stop: stopSequences,
        seed,
        stream: true,
      };

      // Make POST request with SSE response
      const response = await fetch(url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Accept': 'text/event-stream',
        },
        body: JSON.stringify(requestBody),
        signal,
        credentials: 'include', // Use httpOnly session cookie
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
              component: 'useInferenceStream',
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
            if (chunk.id && !responseId && isMountedRef.current) {
              setResponseId(chunk.id);
            }

            // Process each choice
            for (const choice of chunk.choices) {
              const content = choice.delta?.content;

              if (content && isMountedRef.current) {
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
                setTokens((prev) => [...prev, newToken]);

                // Notify token callback
                if (onTokenRef.current) {
                  onTokenRef.current(newToken);
                }

                // Update metrics periodically
                const elapsed = Date.now() - startTimeRef.current;
                if (elapsed > 0) {
                  setLatencyMs(elapsed);
                  setTokensPerSecond((tokenCountRef.current / elapsed) * 1000);
                }
              }

              // Check finish reason
              if (choice.finish_reason && isMountedRef.current) {
                setFinishReason(choice.finish_reason);
              }
            }
          } catch (parseError) {
            logger.warn('Failed to parse SSE chunk', {
              component: 'useInferenceStream',
              operation: 'parse',
              data: eventData.substring(0, 100),
            });
          }
        }
      }

      // Final metrics update
      if (isMountedRef.current) {
        const totalElapsed = Date.now() - startTimeRef.current;
        setLatencyMs(totalElapsed);
        if (totalElapsed > 0 && tokenCountRef.current > 0) {
          setTokensPerSecond((tokenCountRef.current / totalElapsed) * 1000);
        }

        // Call completion callback
        if (onCompleteRef.current) {
          onCompleteRef.current(accumulatedText);
        }
      }

      logger.info('Streaming inference finished', {
        component: 'useInferenceStream',
        operation: 'finish',
        tokenCount: tokenCountRef.current,
        totalMs: Date.now() - startTimeRef.current,
      });
    } catch (err) {
      // Handle abort (user cancellation)
      if (err instanceof Error && err.name === 'AbortError') {
        logger.info('Streaming inference aborted by user', {
          component: 'useInferenceStream',
          operation: 'abort',
        });
        return;
      }

      // Handle other errors
      const errorObj = err instanceof Error ? err : new Error('Unknown streaming error');
      if (isMountedRef.current) {
        setError(errorObj);
        setFinishReason('error');

        // Call error callback
        if (onErrorRef.current) {
          onErrorRef.current(errorObj);
        }
      }

      logger.error(
        'Streaming inference failed',
        {
          component: 'useInferenceStream',
          operation: 'error',
        },
        toError(err)
      );
    } finally {
      if (isMountedRef.current) {
        setIsStreaming(false);
        setConnected(false);
      }
      abortControllerRef.current = null;
    }
  }, [
    enabled,
    prompt,
    model,
    adapters,
    stackId,
    maxTokens,
    temperature,
    topP,
    topK,
    stopSequences,
    seed,
    responseId,
    reset,
  ]);

  // Cleanup on unmount
  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
      // Clean up any active stream
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
        abortControllerRef.current = null;
      }
    };
  }, []);

  return {
    text,
    tokens,
    isStreaming,
    connected,
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

export default useInferenceStream;
