/**
 * Chat Data Mock Factories
 *
 * Factory functions for creating mock ChatSession and ChatMessage data.
 */

import type { ChatSession as LocalChatSession } from '@/types/chat';
import type { ChatMessage as LocalChatMessage } from '@/components/chat/ChatMessage';

/**
 * Create a mock ChatMessage object
 *
 * @example
 * ```typescript
 * const msg = createMockChatMessage(); // Default user message
 * const assistant = createMockChatMessage({ role: 'assistant', content: 'Hello!' });
 * ```
 */
export function createMockChatMessage(
  overrides: Partial<LocalChatMessage> = {}
): LocalChatMessage {
  return {
    id: `msg-${Date.now()}`,
    role: 'user',
    content: 'Test message content',
    timestamp: new Date(),
    ...overrides,
  };
}

/**
 * Create a mock ChatSession object
 *
 * @example
 * ```typescript
 * const session = createMockChatSession(); // Empty session
 * const withMessages = createMockChatSession({
 *   messages: [createMockChatMessage({ content: 'Hi' })],
 * });
 * ```
 */
export function createMockChatSession(
  overrides: Partial<LocalChatSession> = {}
): LocalChatSession {
  return {
    id: 'session-1',
    name: 'Test Session',
    stackId: 'stack-1',
    stackName: 'Test Stack',
    collectionId: null,
    documentId: undefined,
    documentName: undefined,
    sourceType: undefined,
    metadata: undefined,
    messages: [],
    createdAt: new Date(),
    updatedAt: new Date(),
    tenantId: 'test-tenant',
    ...overrides,
  };
}

/**
 * Create a chat session with pre-populated conversation
 *
 * @example
 * ```typescript
 * const session = createMockChatSessionWithConversation(3); // 3 exchanges (6 messages)
 * ```
 */
export function createMockChatSessionWithConversation(
  exchangeCount: number,
  sessionOverrides: Partial<LocalChatSession> = {}
): LocalChatSession {
  const messages: LocalChatMessage[] = [];
  const baseTime = Date.now();

  for (let i = 0; i < exchangeCount; i++) {
    messages.push(
      createMockChatMessage({
        id: `msg-user-${i}`,
        role: 'user',
        content: `User message ${i + 1}`,
        timestamp: new Date(baseTime + i * 2000),
      }),
      createMockChatMessage({
        id: `msg-assistant-${i}`,
        role: 'assistant',
        content: `Assistant response ${i + 1}`,
        timestamp: new Date(baseTime + i * 2000 + 1000),
      })
    );
  }

  return createMockChatSession({
    messages,
    ...sessionOverrides,
  });
}
