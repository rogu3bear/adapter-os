import { describe, it, expect } from 'vitest';
import {
  renderChatSessionMarkdown,
  generateExportFilename,
  type ExportMetadata,
} from '@/utils/export/renderMarkdown';
import type { ChatMessage } from '@/components/chat/ChatMessage';

describe('renderChatSessionMarkdown', () => {
  const mockMetadata: ExportMetadata = {
    exportId: 'test-export-123',
    exportTimestamp: '2025-12-12T10:00:00.000Z',
    entityType: 'chat_session',
    entityId: 'session-123',
    entityName: 'Test Session',
  };

  it('renders basic chat session with metadata', () => {
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'Hello, how are you?',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      },
      {
        id: 'msg-2',
        role: 'assistant',
        content: 'I am doing well, thank you!',
        timestamp: new Date('2025-12-12T09:00:05.000Z'),
      },
    ];

    const markdown = renderChatSessionMarkdown('Test Session', messages, mockMetadata);

    expect(markdown).toContain('# Chat Session: Test Session');
    expect(markdown).toContain('## Metadata');
    expect(markdown).toContain('- **Export Date**: 2025-12-12T10:00:00.000Z');
    expect(markdown).toContain('- **Session ID**: session-123');
    expect(markdown).toContain('## Conversation');
    expect(markdown).toContain('### **You**');
    expect(markdown).toContain('Hello, how are you?');
    expect(markdown).toContain('### **Assistant**');
    expect(markdown).toContain('I am doing well, thank you!');
  });

  it('renders messages with evidence citations', () => {
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'What is in the documentation?',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      },
      {
        id: 'msg-2',
        role: 'assistant',
        content: 'Based on the documentation...',
        timestamp: new Date('2025-12-12T09:00:05.000Z'),
        evidence: [
          {
            document_id: 'doc-1',
            document_name: 'User Guide',
            chunk_id: 'chunk-1',
            page_number: 5,
            text_preview: 'This is a preview of the content',
            relevance_score: 0.95,
            rank: 1,
          },
          {
            document_id: 'doc-2',
            document_name: 'API Reference',
            chunk_id: 'chunk-2',
            page_number: null,
            text_preview: 'API details here',
            relevance_score: 0.87,
            rank: 2,
          },
        ],
      },
    ];

    const markdown = renderChatSessionMarkdown('Test Session', messages, mockMetadata);

    expect(markdown).toContain('**Sources:**');
    expect(markdown).toContain('- User Guide (p.5) [95.0% relevance]');
    expect(markdown).toContain('> "This is a preview of the content"');
    expect(markdown).toContain('- API Reference [87.0% relevance]');
    expect(markdown).toContain('> "API details here"');
  });

  it('renders messages with router decisions', () => {
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'Translate this text',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      },
      {
        id: 'msg-2',
        role: 'assistant',
        content: 'Here is the translation...',
        timestamp: new Date('2025-12-12T09:00:05.000Z'),
        routerDecision: {
          request_id: 'req-123',
          selected_adapters: ['adapter-1', 'adapter-2'],
          scores: {
            'adapter-1': 0.95,
            'adapter-2': 0.87,
          },
          timestamp: '2025-12-12T09:00:05.000Z',
          latency_ms: 50,
          candidates: [
            {
              adapter_id: 'adapter-1',
              adapter_idx: 0,
              gate_q15: 26214,
              gate_float: 0.8,
              raw_score: 0.95,
              selected: true,
              rank: 1,
            },
            {
              adapter_id: 'adapter-2',
              adapter_idx: 1,
              gate_q15: 19661,
              gate_float: 0.6,
              raw_score: 0.87,
              selected: true,
              rank: 2,
            },
          ],
        },
      },
    ];

    const markdown = renderChatSessionMarkdown('Test Session', messages, mockMetadata);

    expect(markdown).toContain('**Adapters Used:**');
    expect(markdown).toContain('- Adapter adapter-1 (weight: 80.0%)');
    expect(markdown).toContain('- Adapter adapter-2 (weight: 60.0%)');
  });

  it('renders verified messages with verification timestamp', () => {
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'Question',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      },
      {
        id: 'msg-2',
        role: 'assistant',
        content: 'Answer',
        timestamp: new Date('2025-12-12T09:00:05.000Z'),
        isVerified: true,
        verifiedAt: '2025-12-12T09:00:10.000Z',
      },
    ];

    const markdown = renderChatSessionMarkdown('Test Session', messages, mockMetadata);

    expect(markdown).toContain('*Verified at');
  });

  it('handles messages without timestamps', () => {
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'Hello',
        timestamp: new Date(),
      },
    ];

    // Manually create a message without proper timestamp
    const messageWithoutTime = { ...messages[0] } as unknown as ChatMessage;
    (messageWithoutTime as { timestamp?: Date }).timestamp = undefined as unknown as Date;

    const markdown = renderChatSessionMarkdown('Test Session', [messageWithoutTime], mockMetadata);

    expect(markdown).toContain('### **You**');
    expect(markdown).toContain('Hello');
  });
});

describe('generateExportFilename', () => {
  it('generates safe filename with timestamp', () => {
    const filename = generateExportFilename('My Chat Session', 'md');

    expect(filename).toMatch(/^my-chat-session-\d{4}-\d{2}-\d{2}\.md$/);
  });

  it('sanitizes special characters', () => {
    const filename = generateExportFilename('Chat@#$%Session!!!', 'json');

    // Special characters are replaced with dashes (may create consecutive dashes)
    expect(filename).toMatch(/^chat-session-+-\d{4}-\d{2}-\d{2}\.json$/);
  });

  it('limits filename length', () => {
    const longName = 'A'.repeat(100);
    const filename = generateExportFilename(longName, 'md');

    // Should be max 50 chars for name + timestamp + extension
    expect(filename.length).toBeLessThan(70);
  });

  it('handles multiple consecutive special characters', () => {
    const filename = generateExportFilename('Chat---Session   Name', 'txt');

    expect(filename).toMatch(/^chat-session-name-\d{4}-\d{2}-\d{2}\.txt$/);
  });
});
