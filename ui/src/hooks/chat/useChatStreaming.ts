import { useState, useRef, useCallback } from 'react';
import { toast } from 'sonner';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import type { ChatMessage } from '@/components/chat/ChatMessage';
import type { StreamingInferRequest } from '@/api/streaming-types';

/**
 * Options for configuring the chat streaming hook.
 */
export interface UseChatStreamingOptions {
  /** Current chat session ID (required for message persistence) */
  sessionId: string | null;

  /** Stack ID to use for inference (adapter IDs will be resolved from stack) */
  stackId?: string;

  /** Collection ID for RAG-enhanced inference */
  collectionId?: string;

  /** Document ID for document-specific chat (not yet supported by API, but stored for future use) */
  documentId?: string;

  /** Callback invoked when a user message is successfully sent */
  onMessageSent?: (message: ChatMessage) => void;

  /** Callback invoked when streaming completes and assistant message is finalized */
  onStreamComplete?: (response: ChatMessage) => void;

  /** Callback invoked when an error occurs during streaming */
  onError?: (error: Error) => void;
}

/**
 * Return value from the chat streaming hook.
 */
export interface UseChatStreamingReturn {
  // State
  /** Whether a streaming request is currently in progress */
  isStreaming: boolean;

  /** The accumulated text from the current stream */
  streamedText: string;

  /** Unique ID for the current request (for correlation with router decisions) */
  currentRequestId: string | null;

  // Actions
  /** Send a message and begin streaming the response */
  sendMessage: (content: string, adapterIds: string[]) => Promise<void>;

  /** Cancel the current streaming request */
  cancelStream: () => void;

  /** Reset streaming state (clears accumulated text and request ID) */
  resetStream: () => void;

  // Metrics
  /** Number of tokens received in the current stream */
  tokensReceived: number;

  /** Duration of the current/last stream in milliseconds */
  streamDuration: number | null;
}

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

    // Validate adapter IDs
    if (!adapterIds || adapterIds.length === 0) {
      toast.error('Please select a stack with adapters');
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
    const requestId = `chat-${Date.now()}`;
    setCurrentRequestId(requestId);

    // Prepare request
    // Note: documentId is accepted in options but not yet supported by StreamingInferRequest API
    // It's stored here for future API support. Currently, collectionId provides document scoping.
    const request: StreamingInferRequest = {
      prompt: validatedContent,
      max_tokens: 500,
      temperature: 0.7,
      adapter_stack: adapterIds,
      ...(collectionId && { collection_id: collectionId }),
      // TODO: Add document_id when API supports it: ...(documentId && { document_id: documentId }),
    };

    try {
      let fullText = '';
      let tokenCount = 0;

      await apiClient.streamInfer(
        request,
        {
          onToken: (token: string) => {
            tokenCount++;
            fullText += token;
            setStreamedText(fullText);
            setTokensReceived(tokenCount);
          },

          onComplete: async (completedText: string, finishReason: string | null) => {
            // Calculate final duration
            const duration = streamStartTimeRef.current
              ? Date.now() - streamStartTimeRef.current
              : null;
            setStreamDuration(duration);

            // Create completed assistant message
            const assistantMessage: ChatMessage = {
              id: `assistant-${Date.now()}`,
              role: 'assistant',
              content: completedText,
              timestamp: new Date(),
              requestId,
              isStreaming: false,
            };

            logger.info('Stream completed', {
              component: 'useChatStreaming',
              requestId,
              tokensReceived: tokenCount,
              duration,
              finishReason,
              sessionId: sessionId ?? undefined,
            });

            setIsStreaming(false);
            setCurrentRequestId(null);

            // Notify completion
            onStreamComplete?.(assistantMessage);
          },

          onError: (error: Error) => {
            logger.error('Stream error', {
              component: 'useChatStreaming',
              requestId,
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
          requestId,
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

    // Actions
    sendMessage,
    cancelStream,
    resetStream,

    // Metrics
    tokensReceived,
    streamDuration,
  };
}
