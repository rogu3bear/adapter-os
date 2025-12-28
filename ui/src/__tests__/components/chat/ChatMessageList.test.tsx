import { render, screen, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';
import React, { createRef } from 'react';
import { ChatMessageList, type ChatMessageListRef, type StreamingChunk } from '@/components/chat/ChatMessageList';
import type { ChatMessage } from '@/types/components';

// Mock @tanstack/react-virtual
const mockScrollToIndex = vi.fn();
const mockGetVirtualItems = vi.fn();
const mockGetTotalSize = vi.fn();
const mockMeasureElement = vi.fn();

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: vi.fn(() => ({
    scrollToIndex: mockScrollToIndex,
    getVirtualItems: mockGetVirtualItems,
    getTotalSize: mockGetTotalSize,
    measureElement: mockMeasureElement,
  })),
}));

// Mock ChatMessageComponent to simplify testing
vi.mock('@/components/chat/ChatMessage', () => ({
  ChatMessageComponent: vi.fn(({ message, onSelect, isSelected }) => (
    <div
      data-testid={`chat-message-${message.id}`}
      data-selected={isSelected}
      onClick={() => onSelect?.(message.id, message.traceId)}
      role="article"
    >
      <span data-testid="message-role">{message.role}</span>
      <span data-testid="message-content">{message.content}</span>
      {message.isStreaming && <span data-testid="streaming-indicator">Streaming...</span>}
    </div>
  )),
}));

// Mock RunEvidencePanel
vi.mock('@/components/chat/RunEvidencePanel', () => ({
  RunEvidencePanel: vi.fn(({ pending }) => (
    <div data-testid="run-evidence-panel" data-pending={pending}>
      Run Evidence Panel
    </div>
  )),
}));

// Helper to create mock messages
function createMockMessage(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id: `msg-${Math.random().toString(36).slice(2, 9)}`,
    role: 'assistant',
    content: 'Test message content',
    timestamp: new Date(),
    requestId: 'req-123',
    traceId: 'trace-123',
    isStreaming: false,
    ...overrides,
  };
}

// Helper to create mock streaming chunks
function createMockChunks(count: number): StreamingChunk[] {
  return Array.from({ length: count }, (_, i) => ({
    token: `token-${i}`,
    content: `content-${i}`,
    timestamp: Date.now() + i * 100,
    index: i,
    logprob: -0.5,
    routerScore: 0.8,
  }));
}

describe('ChatMessageList', () => {
  let scrollAreaRef: React.RefObject<HTMLDivElement>;
  const defaultProps = {
    streamingMessageId: null,
    selectedMessageId: null,
    streamingContent: '',
    chunks: [] as StreamingChunk[],
    onSelectMessage: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers({ shouldAdvanceTime: true });

    // Create a fresh ref for each test
    scrollAreaRef = createRef<HTMLDivElement>();

    // Default virtualizer mock behavior
    mockGetTotalSize.mockReturnValue(300);
    mockGetVirtualItems.mockReturnValue([]);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('Empty message list', () => {
    it('renders empty state when no messages provided', () => {
      render(
        <ChatMessageList
          {...defaultProps}
          messages={[]}
          scrollAreaRef={scrollAreaRef}
        />
      );

      expect(screen.getByRole('status')).toBeInTheDocument();
      expect(screen.getByText('Start a conversation')).toBeInTheDocument();
      expect(screen.getByText(/Select a stack and send a message to begin/)).toBeInTheDocument();
    });

    it('renders document context message when documentContext is provided', () => {
      render(
        <ChatMessageList
          {...defaultProps}
          messages={[]}
          scrollAreaRef={scrollAreaRef}
          documentContext={{
            documentId: 'doc-123',
            documentName: 'Test Document.pdf',
          }}
        />
      );

      expect(screen.getByText('Start a conversation')).toBeInTheDocument();
      expect(screen.getByText(/I'm ready to help you with "Test Document.pdf"/)).toBeInTheDocument();
    });

    it('renders dataset context message when datasetContext is provided', () => {
      render(
        <ChatMessageList
          {...defaultProps}
          messages={[]}
          scrollAreaRef={scrollAreaRef}
          datasetContext={{
            datasetId: 'ds-123',
            datasetName: 'Test Dataset',
          }}
        />
      );

      expect(screen.getByText('Start a conversation')).toBeInTheDocument();
      expect(screen.getByText(/I'm ready to help you with the "Test Dataset" dataset/)).toBeInTheDocument();
    });
  });

  describe('Message rendering', () => {
    it('renders messages correctly via virtualizer', () => {
      const messages = [
        createMockMessage({ id: 'msg-1', content: 'First message', role: 'user' }),
        createMockMessage({ id: 'msg-2', content: 'Second message', role: 'assistant' }),
      ];

      // Mock virtual items returned by the virtualizer
      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
        { index: 1, start: 150, size: 150, key: 'msg-2' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
        />
      );

      expect(screen.getByTestId('chat-message-msg-1')).toBeInTheDocument();
      expect(screen.getByTestId('chat-message-msg-2')).toBeInTheDocument();
    });

    it('renders RunEvidencePanel for assistant messages', () => {
      const messages = [
        createMockMessage({ id: 'msg-1', role: 'assistant', content: 'Assistant response' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
        />
      );

      expect(screen.getByTestId('run-evidence-panel')).toBeInTheDocument();
    });

    it('does not render RunEvidencePanel for user messages', () => {
      const messages = [
        createMockMessage({ id: 'msg-1', role: 'user', content: 'User message' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
        />
      );

      expect(screen.queryByTestId('run-evidence-panel')).not.toBeInTheDocument();
    });

    it('applies correct positioning styles from virtualizer', () => {
      const messages = [
        createMockMessage({ id: 'msg-1', content: 'Test' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 100, size: 150, key: 'msg-1' },
      ]);
      mockGetTotalSize.mockReturnValue(250);

      const { container } = render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
        />
      );

      // Check the virtual container has correct height
      const virtualContainer = container.querySelector('[style*="height: 250px"]');
      expect(virtualContainer).toBeInTheDocument();
    });
  });

  describe('Message selection', () => {
    it('calls onSelectMessage when a message is clicked', async () => {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      const onSelectMessage = vi.fn();
      const messages = [
        createMockMessage({ id: 'msg-1', traceId: 'trace-abc' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          onSelectMessage={onSelectMessage}
        />
      );

      await user.click(screen.getByTestId('chat-message-msg-1'));

      expect(onSelectMessage).toHaveBeenCalledWith('msg-1', 'trace-abc');
    });

    it('highlights selected message', () => {
      const messages = [
        createMockMessage({ id: 'msg-1' }),
        createMockMessage({ id: 'msg-2' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
        { index: 1, start: 150, size: 150, key: 'msg-2' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          selectedMessageId="msg-1"
        />
      );

      expect(screen.getByTestId('chat-message-msg-1')).toHaveAttribute('data-selected', 'true');
      expect(screen.getByTestId('chat-message-msg-2')).toHaveAttribute('data-selected', 'false');
    });
  });

  describe('Streaming messages', () => {
    it('updates streaming message with current streaming content', () => {
      const messages = [
        createMockMessage({
          id: 'msg-streaming',
          content: 'Initial content',
          isStreaming: true,
        }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-streaming' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          streamingMessageId="msg-streaming"
          streamingContent="Updated streaming content..."
        />
      );

      // The ChatMessageComponent should receive the updated content
      expect(screen.getByTestId('message-content')).toHaveTextContent('Updated streaming content...');
    });

    it('passes streaming chunks as tokenStream to streaming message', () => {
      const messages = [
        createMockMessage({
          id: 'msg-streaming',
          isStreaming: true,
        }),
      ];
      const chunks = createMockChunks(5);

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-streaming' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          streamingMessageId="msg-streaming"
          streamingContent="Streaming..."
          chunks={chunks}
        />
      );

      // Verify the component renders (the actual tokenStream handling is in ChatMessageComponent)
      expect(screen.getByTestId('chat-message-msg-streaming')).toBeInTheDocument();
    });

    it('shows streaming indicator on streaming message', () => {
      const messages = [
        createMockMessage({
          id: 'msg-streaming',
          isStreaming: true,
        }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-streaming' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          streamingMessageId="msg-streaming"
          streamingContent="Loading..."
        />
      );

      expect(screen.getByTestId('streaming-indicator')).toBeInTheDocument();
    });

    it('marks RunEvidencePanel as pending for streaming messages', () => {
      const messages = [
        createMockMessage({
          id: 'msg-streaming',
          role: 'assistant',
          isStreaming: true,
        }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-streaming' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          streamingMessageId="msg-streaming"
          streamingContent="Streaming..."
        />
      );

      expect(screen.getByTestId('run-evidence-panel')).toHaveAttribute('data-pending', 'true');
    });
  });

  describe('Auto-scroll behavior', () => {
    it('scrolls to bottom when new messages arrive', async () => {
      const messages = [
        createMockMessage({ id: 'msg-1' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
        />
      );

      // Advance timers to trigger the auto-scroll effect
      await act(async () => {
        vi.advanceTimersByTime(150);
      });

      expect(mockScrollToIndex).toHaveBeenCalledWith(0, {
        align: 'end',
        behavior: 'smooth',
      });
    });

    it('scrolls to latest message when messages array grows', async () => {
      const initialMessages = [
        createMockMessage({ id: 'msg-1' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      const { rerender } = render(
        <ChatMessageList
          {...defaultProps}
          messages={initialMessages}
          scrollAreaRef={scrollAreaRef}
        />
      );

      // Clear previous calls
      mockScrollToIndex.mockClear();

      // Add new message
      const updatedMessages = [
        ...initialMessages,
        createMockMessage({ id: 'msg-2' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
        { index: 1, start: 150, size: 150, key: 'msg-2' },
      ]);

      rerender(
        <ChatMessageList
          {...defaultProps}
          messages={updatedMessages}
          scrollAreaRef={scrollAreaRef}
        />
      );

      await act(async () => {
        vi.advanceTimersByTime(150);
      });

      // Should scroll to the last message (index 1)
      expect(mockScrollToIndex).toHaveBeenCalledWith(1, {
        align: 'end',
        behavior: 'smooth',
      });
    });
  });

  describe('scrollToBottom ref method', () => {
    it('exposes scrollToBottom method via ref', async () => {
      const listRef = createRef<ChatMessageListRef>();
      const messages = [
        createMockMessage({ id: 'msg-1' }),
        createMockMessage({ id: 'msg-2' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
        { index: 1, start: 150, size: 150, key: 'msg-2' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          ref={listRef}
        />
      );

      expect(listRef.current).not.toBeNull();
      expect(listRef.current?.scrollToBottom).toBeInstanceOf(Function);

      // Clear previous calls
      mockScrollToIndex.mockClear();

      // Call scrollToBottom
      act(() => {
        listRef.current?.scrollToBottom();
      });

      expect(mockScrollToIndex).toHaveBeenCalledWith(1, {
        align: 'end',
        behavior: 'smooth',
      });
    });

    it('does nothing when called with no messages', () => {
      const listRef = createRef<ChatMessageListRef>();

      render(
        <ChatMessageList
          {...defaultProps}
          messages={[]}
          scrollAreaRef={scrollAreaRef}
          ref={listRef}
        />
      );

      mockScrollToIndex.mockClear();

      // Should not throw when called with empty messages
      act(() => {
        listRef.current?.scrollToBottom();
      });

      // scrollToIndex should not be called since there are no messages
      expect(mockScrollToIndex).not.toHaveBeenCalled();
    });
  });

  describe('Loading decision state', () => {
    it('shows loading decision indicator when isLoadingDecision is true', () => {
      const messages = [
        createMockMessage({ id: 'msg-1' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          isLoadingDecision={true}
        />
      );

      expect(screen.getByText('Loading router decision details...')).toBeInTheDocument();
      expect(screen.getByRole('status')).toBeInTheDocument();
    });

    it('does not show loading indicator when isLoadingDecision is false', () => {
      const messages = [
        createMockMessage({ id: 'msg-1' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          isLoadingDecision={false}
        />
      );

      expect(screen.queryByText('Loading router decision details...')).not.toBeInTheDocument();
    });
  });

  describe('Developer and kernel modes', () => {
    it('passes developerMode prop to ChatMessageComponent', () => {
      const messages = [
        createMockMessage({ id: 'msg-1' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          developerMode={true}
        />
      );

      // ChatMessageComponent is mocked, so we just verify render succeeds
      expect(screen.getByTestId('chat-message-msg-1')).toBeInTheDocument();
    });

    it('passes kernelMode prop to ChatMessageComponent', () => {
      const messages = [
        createMockMessage({ id: 'msg-1' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          kernelMode={true}
        />
      );

      expect(screen.getByTestId('chat-message-msg-1')).toBeInTheDocument();
    });
  });

  describe('Workspace and tenant context', () => {
    it('passes workspaceActiveState to RunEvidencePanel', () => {
      const messages = [
        createMockMessage({ id: 'msg-1', role: 'assistant' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          workspaceActiveState={{
            activePlanId: 'plan-123',
            manifestHashB3: 'manifest-abc',
          }}
          tenantId="tenant-456"
        />
      );

      expect(screen.getByTestId('run-evidence-panel')).toBeInTheDocument();
    });
  });

  describe('Callbacks', () => {
    it('passes onViewDocument callback', () => {
      const onViewDocument = vi.fn();
      const messages = [
        createMockMessage({ id: 'msg-1' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          onViewDocument={onViewDocument}
        />
      );

      expect(screen.getByTestId('chat-message-msg-1')).toBeInTheDocument();
    });

    it('passes onExportRunEvidence callback to RunEvidencePanel', () => {
      const onExportRunEvidence = vi.fn();
      const messages = [
        createMockMessage({ id: 'msg-1', role: 'assistant' }),
      ];

      mockGetVirtualItems.mockReturnValue([
        { index: 0, start: 0, size: 150, key: 'msg-1' },
      ]);

      render(
        <ChatMessageList
          {...defaultProps}
          messages={messages}
          scrollAreaRef={scrollAreaRef}
          onExportRunEvidence={onExportRunEvidence}
        />
      );

      expect(screen.getByTestId('run-evidence-panel')).toBeInTheDocument();
    });
  });
});
