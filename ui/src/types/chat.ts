// Chat session types for persistence
// 【2025-01-20†rectification†chat_session_types】

import type { ChatMessage } from '@/components/chat/ChatMessage';

export interface ChatSession {
  id: string;
  name: string;
  stackId: string;
  stackName?: string;
  messages: ChatMessage[];
  createdAt: Date;
  updatedAt: Date;
  tenantId: string;
}

// Storage key format: `chat_sessions_${tenantId}`
export function getStorageKey(tenantId: string): string {
  return `chat_sessions_${tenantId}`;
}

// Serialize session for storage (convert Date to ISO string)
export function serializeSession(session: ChatSession): string {
  return JSON.stringify({
    ...session,
    createdAt: session.createdAt.toISOString(),
    updatedAt: session.updatedAt.toISOString(),
    messages: session.messages.map(msg => ({
      ...msg,
      timestamp: msg.timestamp.toISOString(),
    })),
  });
}

// Deserialize session from storage (convert ISO string to Date)
export function deserializeSession(data: string): ChatSession {
  const parsed = JSON.parse(data);
  return {
    ...parsed,
    createdAt: new Date(parsed.createdAt),
    updatedAt: new Date(parsed.updatedAt),
    messages: parsed.messages.map((msg: { timestamp: string; [key: string]: unknown }) => ({
      ...msg,
      timestamp: new Date(msg.timestamp),
    })),
  };
}

