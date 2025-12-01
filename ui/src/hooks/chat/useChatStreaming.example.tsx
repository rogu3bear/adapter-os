/**
 * Example: Refactoring ChatInterface to use useChatStreaming hook
 *
 * This shows how to migrate from inline streaming logic to the hook.
 */

import { useState } from 'react';
import { useChatStreaming } from './useChatStreaming';
import type { ChatMessage } from '@/components/chat/ChatMessage';

/**
 * BEFORE: Inline streaming logic (150+ lines in ChatInterface.tsx)
 *
 * Problems:
 * - Complex state management scattered across component
 * - Hard to test streaming logic in isolation
 * - Difficult to reuse in other components
 * - Mixed concerns (UI + business logic)
 */

/**
 * AFTER: Using useChatStreaming hook
 *
 * Benefits:
 * - Clean separation of concerns
 * - Reusable across components
 * - Easier to test
 * - Better type safety
 * - Clearer error handling
 */

export function ChatInterfaceExample() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [currentSessionId] = useState('session-123');
  const selectedAdapterIds = ['adapter-1', 'adapter-2']; // From stack

  // Hook handles all streaming complexity
  const {
    isStreaming,
    streamedText,
    sendMessage,
    cancelStream,
    tokensReceived,
    streamDuration,
  } = useChatStreaming({
    sessionId: currentSessionId,
    collectionId: 'collection-456',

    onMessageSent: (userMessage) => {
      // Add user message to UI
      setMessages(prev => [...prev, userMessage]);
      // eslint-disable-next-line no-console -- example file
      console.log('Message sent:', userMessage);
    },

    onStreamComplete: (assistantMessage) => {
      // Replace streaming placeholder with final message
      setMessages(prev => {
        const withoutStreaming = prev.filter(m => !m.isStreaming);
        return [...withoutStreaming, assistantMessage];
      });
      // eslint-disable-next-line no-console -- example file
      console.log('Stream complete:', assistantMessage);
    },

    onError: (error) => {
      // Remove failed message from UI
      setMessages(prev => prev.filter(m => !m.isStreaming));
      console.error('Stream error:', error);
    },
  });

  // Simple send handler (was 150+ lines, now ~20)
  const handleSend = async () => {
    if (!input.trim() || isStreaming) return;

    // Hook handles validation, streaming, error handling
    await sendMessage(input, selectedAdapterIds);

    // Clear input
    setInput('');
  };

  // Show streaming placeholder in messages
  const displayMessages = isStreaming
    ? [
        ...messages,
        {
          id: 'streaming',
          role: 'assistant' as const,
          content: streamedText,
          timestamp: new Date(),
          isStreaming: true,
        },
      ]
    : messages;

  return (
    <div className="flex flex-col h-full">
      {/* Messages */}
      <div className="flex-1 overflow-y-auto">
        {displayMessages.map(msg => (
          <div key={msg.id}>
            <strong>{msg.role}:</strong> {msg.content}
          </div>
        ))}
      </div>

      {/* Metrics */}
      {isStreaming && (
        <div className="text-sm text-gray-500">
          Tokens: {tokensReceived} | Duration: {streamDuration ?? 0}ms
        </div>
      )}

      {/* Input */}
      <div className="flex gap-2">
        <input
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyPress={e => e.key === 'Enter' && handleSend()}
          disabled={isStreaming}
          placeholder="Type a message..."
          className="flex-1 border rounded px-3 py-2"
        />

        <button
          onClick={handleSend}
          disabled={isStreaming || !input.trim()}
          className="px-4 py-2 bg-blue-500 text-white rounded disabled:opacity-50"
        >
          {isStreaming ? 'Sending...' : 'Send'}
        </button>

        {isStreaming && (
          <button
            onClick={cancelStream}
            className="px-4 py-2 bg-red-500 text-white rounded"
          >
            Cancel
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * Migration Guide for ChatInterface.tsx
 *
 * 1. Import the hook:
 *    import { useChatStreaming } from '@/hooks/chat';
 *
 * 2. Replace state variables:
 *    REMOVE: isLoading, currentRequestId, abortControllerRef
 *    USE: isStreaming, currentRequestId from hook
 *
 * 3. Replace handleSend function:
 *    REMOVE: Lines 423-571 (entire handleSend implementation)
 *    USE: sendMessage from hook + callbacks
 *
 * 4. Update message display:
 *    REMOVE: Manual streaming message state management
 *    USE: Hook's streamedText + isStreaming state
 *
 * 5. Handle cancellation:
 *    REMOVE: Manual AbortController management
 *    USE: cancelStream from hook
 *
 * Example refactored handleSend:
 *
 * const handleSend = useCallback(async () => {
 *   if (!input.trim() || isStreaming) return;
 *
 *   // Check adapters ready
 *   if (!allAdaptersReady && adapterStates.size > 0) {
 *     setPendingMessage(input.trim());
 *     setShowAdapterPrompt(true);
 *     return;
 *   }
 *
 *   const adapterIds = selectedStack?.adapter_ids || [];
 *   if (adapterIds.length === 0) {
 *     toast.error('Please select a stack with adapters');
 *     return;
 *   }
 *
 *   await sendMessage(input, adapterIds);
 *   setInput('');
 * }, [input, isStreaming, selectedStack, allAdaptersReady, sendMessage]);
 */
