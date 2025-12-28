/**
 * ChatHistorySidebar Component Tests
 *
 * Tests for the chat history sidebar component that displays and manages
 * chat session history including search, filtering, renaming, and deletion.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatHistorySidebar } from '@/components/chat/ChatHistorySidebar';
import type { ChatHistorySidebarProps } from '@/components/chat/ChatHistorySidebar';
import { createMockChatSession, createMockChatMessage } from '@/test/mocks/data/chat';
import type { ChatSession } from '@/types/chat';

// Mock logger
vi.mock('@/utils/logger', () => ({
  logger: {
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock ChatSearchBar to avoid complex hook dependencies
vi.mock('@/components/chat/ChatSearchBar', () => ({
  ChatSearchBar: ({
    value,
    onChange,
    placeholder,
  }: {
    value?: string;
    onChange?: (value: string) => void;
    placeholder?: string;
  }) => (
    <input
      data-testid="chat-search-bar"
      value={value}
      onChange={(e) => onChange?.(e.target.value)}
      placeholder={placeholder}
      aria-label="Search sessions"
    />
  ),
}));

// Mock ChatSessionActions to avoid complex hook dependencies
vi.mock('@/components/chat/ChatSessionActions', () => ({
  ChatSessionActions: ({
    sessionId,
    onRename,
    onManageTags,
    onSetCategory,
    onShare,
  }: {
    sessionId: string;
    onRename: () => void;
    onManageTags: () => void;
    onSetCategory: () => void;
    onShare: () => void;
  }) => (
    <button
      data-testid={`session-actions-${sessionId}`}
      onClick={(e) => {
        e.stopPropagation();
        // Expose callbacks via data attributes for testing
      }}
      aria-label="Session actions"
    >
      <span data-testid={`rename-trigger-${sessionId}`} onClick={onRename}>
        Rename
      </span>
      <span data-testid={`tags-trigger-${sessionId}`} onClick={onManageTags}>
        Tags
      </span>
      <span data-testid={`category-trigger-${sessionId}`} onClick={onSetCategory}>
        Category
      </span>
      <span data-testid={`share-trigger-${sessionId}`} onClick={onShare}>
        Share
      </span>
    </button>
  ),
}));

// Mock SectionErrorBoundary
vi.mock('@/components/ui/section-error-boundary', () => ({
  SectionErrorBoundary: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

// Mock ScrollArea
vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: ({ children, className }: { children: React.ReactNode; className?: string }) => (
    <div className={className} data-testid="scroll-area">
      {children}
    </div>
  ),
}));

/**
 * Helper function to create default props for ChatHistorySidebar
 */
function createDefaultProps(overrides: Partial<ChatHistorySidebarProps> = {}): ChatHistorySidebarProps {
  return {
    sessions: [],
    activeSessionId: null,
    isOpen: true,
    tenantId: 'test-tenant',
    hasSelectedStack: true,
    onClose: vi.fn(),
    onLoadSession: vi.fn(),
    onCreateSession: vi.fn(),
    onDeleteSession: vi.fn(),
    onRenameSession: vi.fn(),
    onOpenArchive: vi.fn(),
    onManageTags: vi.fn(),
    onSetCategory: vi.fn(),
    onShare: vi.fn(),
    ...overrides,
  };
}

/**
 * Helper to create multiple mock sessions
 */
function createMockSessions(count: number): ChatSession[] {
  return Array.from({ length: count }, (_, i) =>
    createMockChatSession({
      id: `session-${i + 1}`,
      name: `Session ${i + 1}`,
      messages: [
        createMockChatMessage({
          id: `msg-${i + 1}`,
          role: 'user',
          content: `Message content for session ${i + 1}`,
        }),
      ],
      updatedAt: new Date(Date.now() - i * 1000 * 60 * 60), // Each session 1 hour apart
    })
  );
}

describe('ChatHistorySidebar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('rendering', () => {
    it('renders nothing when isOpen is false', () => {
      const props = createDefaultProps({ isOpen: false });
      const { container } = render(<ChatHistorySidebar {...props} />);

      expect(container.firstChild).toBeNull();
    });

    it('renders sidebar when isOpen is true', () => {
      const props = createDefaultProps({ isOpen: true });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByText('Conversation History')).toBeInTheDocument();
    });

    it('renders header with close and archive buttons', () => {
      const props = createDefaultProps();
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByRole('button', { name: /close history/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /open archive/i })).toBeInTheDocument();
    });

    it('renders search bar', () => {
      const props = createDefaultProps();
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByTestId('chat-search-bar')).toBeInTheDocument();
    });

    it('renders new session button', () => {
      const props = createDefaultProps();
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByRole('button', { name: /new session/i })).toBeInTheDocument();
    });

    it('disables new session button when hasSelectedStack is false', () => {
      const props = createDefaultProps({ hasSelectedStack: false });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByRole('button', { name: /new session/i })).toBeDisabled();
    });
  });

  describe('sessions list display', () => {
    it('renders sessions list', () => {
      const sessions = createMockSessions(3);
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByText('Session 1')).toBeInTheDocument();
      expect(screen.getByText('Session 2')).toBeInTheDocument();
      expect(screen.getByText('Session 3')).toBeInTheDocument();
    });

    it('displays session preview from first user message', () => {
      const sessions = [
        createMockChatSession({
          id: 'session-1',
          name: 'Test Session',
          messages: [
            createMockChatMessage({
              role: 'user',
              content: 'Hello, this is my first message to the assistant',
            }),
          ],
        }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByText(/Hello, this is my first message to the/)).toBeInTheDocument();
    });

    it('displays message count for each session', () => {
      const sessions = [
        createMockChatSession({
          id: 'session-1',
          name: 'Test Session',
          messages: [
            createMockChatMessage({ id: 'msg-1' }),
            createMockChatMessage({ id: 'msg-2' }),
            createMockChatMessage({ id: 'msg-3' }),
          ],
        }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByText('3 messages')).toBeInTheDocument();
    });

    it('displays singular "message" for sessions with 1 message', () => {
      const sessions = [
        createMockChatSession({
          id: 'session-1',
          name: 'Test Session',
          messages: [createMockChatMessage({ id: 'msg-1' })],
        }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByText('1 message')).toBeInTheDocument();
    });

    it('highlights active session', () => {
      const sessions = createMockSessions(3);
      const props = createDefaultProps({
        sessions,
        activeSessionId: 'session-2',
      });
      render(<ChatHistorySidebar {...props} />);

      // The active session should have specific styling classes
      const sessionItems = screen.getAllByText(/Session \d/);
      const activeSessionItem = sessionItems[1].closest('[class*="rounded-lg"]');
      expect(activeSessionItem).toHaveClass('bg-muted');
    });
  });

  describe('empty states', () => {
    it('shows empty state when no sessions exist', () => {
      const props = createDefaultProps({ sessions: [] });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByText('No conversation history')).toBeInTheDocument();
    });

    it('shows loading state when isLoadingSessions is true', () => {
      const props = createDefaultProps({ isLoadingSessions: true });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByText('Loading sessions...')).toBeInTheDocument();
    });

    it('shows "No matching sessions" when search returns no results', async () => {
      const user = userEvent.setup();
      const sessions = createMockSessions(3);
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      const searchInput = screen.getByTestId('chat-search-bar');
      await user.type(searchInput, 'nonexistent query xyz');

      expect(screen.getByText('No matching sessions')).toBeInTheDocument();
    });
  });

  describe('search/filter functionality', () => {
    it('filters sessions based on search query matching session name', async () => {
      const user = userEvent.setup();
      const sessions = [
        createMockChatSession({ id: 'session-1', name: 'Alpha Session' }),
        createMockChatSession({ id: 'session-2', name: 'Beta Session' }),
        createMockChatSession({ id: 'session-3', name: 'Gamma Session' }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      const searchInput = screen.getByTestId('chat-search-bar');
      await user.type(searchInput, 'Beta');

      expect(screen.queryByText('Alpha Session')).not.toBeInTheDocument();
      expect(screen.getByText('Beta Session')).toBeInTheDocument();
      expect(screen.queryByText('Gamma Session')).not.toBeInTheDocument();
    });

    it('filters sessions based on search query matching message content', async () => {
      const user = userEvent.setup();
      const sessions = [
        createMockChatSession({
          id: 'session-1',
          name: 'Session One',
          messages: [createMockChatMessage({ content: 'Hello world' })],
        }),
        createMockChatSession({
          id: 'session-2',
          name: 'Session Two',
          messages: [createMockChatMessage({ content: 'Goodbye universe' })],
        }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      const searchInput = screen.getByTestId('chat-search-bar');
      await user.type(searchInput, 'universe');

      expect(screen.queryByText('Session One')).not.toBeInTheDocument();
      expect(screen.getByText('Session Two')).toBeInTheDocument();
    });

    it('shows all sessions when search is cleared', async () => {
      const user = userEvent.setup();
      const sessions = createMockSessions(3);
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      const searchInput = screen.getByTestId('chat-search-bar');

      // Type to filter
      await user.type(searchInput, 'Session 1');
      expect(screen.queryByText('Session 2')).not.toBeInTheDocument();

      // Clear the search
      await user.clear(searchInput);

      expect(screen.getByText('Session 1')).toBeInTheDocument();
      expect(screen.getByText('Session 2')).toBeInTheDocument();
      expect(screen.getByText('Session 3')).toBeInTheDocument();
    });

    it('performs case-insensitive search', async () => {
      const user = userEvent.setup();
      const sessions = [
        createMockChatSession({ id: 'session-1', name: 'UPPERCASE Session' }),
        createMockChatSession({ id: 'session-2', name: 'lowercase session' }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      const searchInput = screen.getByTestId('chat-search-bar');
      await user.type(searchInput, 'LOWERCASE');

      expect(screen.queryByText('UPPERCASE Session')).not.toBeInTheDocument();
      expect(screen.getByText('lowercase session')).toBeInTheDocument();
    });
  });

  describe('session interactions', () => {
    it('calls onLoadSession when a session is clicked', async () => {
      const user = userEvent.setup();
      const onLoadSession = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onLoadSession });
      render(<ChatHistorySidebar {...props} />);

      await user.click(screen.getByText('Session 1'));

      expect(onLoadSession).toHaveBeenCalledWith('session-1');
    });

    it('calls onCreateSession when new session button is clicked', async () => {
      const user = userEvent.setup();
      const onCreateSession = vi.fn();
      const props = createDefaultProps({ onCreateSession });
      render(<ChatHistorySidebar {...props} />);

      await user.click(screen.getByRole('button', { name: /new session/i }));

      expect(onCreateSession).toHaveBeenCalled();
    });

    it('calls onClose when close button is clicked', async () => {
      const user = userEvent.setup();
      const onClose = vi.fn();
      const props = createDefaultProps({ onClose });
      render(<ChatHistorySidebar {...props} />);

      await user.click(screen.getByRole('button', { name: /close history/i }));

      expect(onClose).toHaveBeenCalled();
    });

    it('calls onOpenArchive when archive button is clicked', async () => {
      const user = userEvent.setup();
      const onOpenArchive = vi.fn();
      const props = createDefaultProps({ onOpenArchive });
      render(<ChatHistorySidebar {...props} />);

      await user.click(screen.getByRole('button', { name: /open archive/i }));

      expect(onOpenArchive).toHaveBeenCalled();
    });
  });

  describe('session deletion', () => {
    it('calls onDeleteSession when delete button is clicked', async () => {
      const user = userEvent.setup();
      const onDeleteSession = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onDeleteSession });
      render(<ChatHistorySidebar {...props} />);

      const deleteButton = screen.getByRole('button', {
        name: /delete session session 1/i,
      });
      await user.click(deleteButton);

      expect(onDeleteSession).toHaveBeenCalledWith('session-1', expect.any(Object));
    });

    it('does not trigger onLoadSession when delete is clicked', async () => {
      const user = userEvent.setup();
      const onLoadSession = vi.fn();
      const onDeleteSession = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onLoadSession, onDeleteSession });
      render(<ChatHistorySidebar {...props} />);

      const deleteButton = screen.getByRole('button', {
        name: /delete session session 1/i,
      });
      await user.click(deleteButton);

      // onDeleteSession should be called
      expect(onDeleteSession).toHaveBeenCalled();
      // onLoadSession should NOT be called (click was stopped)
      expect(onLoadSession).not.toHaveBeenCalled();
    });
  });

  describe('session rename flow', () => {
    /**
     * Helper to get the session edit input (not the search bar)
     */
    const getEditInput = () => {
      // Get all textboxes and find the one that's not the search bar
      const textboxes = screen.getAllByRole('textbox');
      return textboxes.find(
        (el) => !el.getAttribute('data-testid')?.includes('chat-search-bar')
      );
    };

    it('shows edit input when rename is triggered', async () => {
      const user = userEvent.setup();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      // Initially only search bar should be present
      expect(screen.getAllByRole('textbox')).toHaveLength(1);

      // Click the rename trigger from the mocked ChatSessionActions
      const renameTrigger = screen.getByTestId('rename-trigger-session-1');
      await user.click(renameTrigger);

      // Now we should have 2 textboxes (search + edit)
      expect(screen.getAllByRole('textbox')).toHaveLength(2);

      // The edit input should have the current session name
      const editInput = getEditInput();
      expect(editInput).toBeInTheDocument();
      expect(editInput).toHaveValue('Session 1');
    });

    it('calls onRenameSession when edit is completed with Enter', async () => {
      const user = userEvent.setup();
      const onRenameSession = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onRenameSession });
      render(<ChatHistorySidebar {...props} />);

      // Start editing
      const renameTrigger = screen.getByTestId('rename-trigger-session-1');
      await user.click(renameTrigger);

      // Clear and type new name
      const editInput = getEditInput()!;
      await user.clear(editInput);
      await user.type(editInput, 'New Session Name{Enter}');

      expect(onRenameSession).toHaveBeenCalledWith('session-1', 'New Session Name');
    });

    it('calls onRenameSession when edit input loses focus', async () => {
      const user = userEvent.setup();
      const onRenameSession = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onRenameSession });
      render(<ChatHistorySidebar {...props} />);

      // Start editing
      const renameTrigger = screen.getByTestId('rename-trigger-session-1');
      await user.click(renameTrigger);

      // Clear and type new name, then blur
      const editInput = getEditInput()!;
      await user.clear(editInput);
      await user.type(editInput, 'Blurred Name');

      // Simulate blur by clicking elsewhere
      fireEvent.blur(editInput);

      expect(onRenameSession).toHaveBeenCalledWith('session-1', 'Blurred Name');
    });

    it('cancels edit when Escape is pressed', async () => {
      const user = userEvent.setup();
      const onRenameSession = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onRenameSession });
      render(<ChatHistorySidebar {...props} />);

      // Start editing
      const renameTrigger = screen.getByTestId('rename-trigger-session-1');
      await user.click(renameTrigger);

      // Type something then press Escape
      const editInput = getEditInput()!;
      await user.type(editInput, 'Some Text{Escape}');

      // Should not call rename
      expect(onRenameSession).not.toHaveBeenCalled();

      // Edit input should be gone, leaving only search bar
      expect(screen.getAllByRole('textbox')).toHaveLength(1);
    });

    it('does not rename with empty string', async () => {
      const user = userEvent.setup();
      const onRenameSession = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onRenameSession });
      render(<ChatHistorySidebar {...props} />);

      // Start editing
      const renameTrigger = screen.getByTestId('rename-trigger-session-1');
      await user.click(renameTrigger);

      // Clear input and press Enter
      const editInput = getEditInput()!;
      await user.clear(editInput);
      await user.keyboard('{Enter}');

      // Should not call rename with empty string
      expect(onRenameSession).not.toHaveBeenCalled();
    });

    it('does not trigger session load when clicking inside edit input', async () => {
      const user = userEvent.setup();
      const onLoadSession = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onLoadSession });
      render(<ChatHistorySidebar {...props} />);

      // Start editing
      const renameTrigger = screen.getByTestId('rename-trigger-session-1');
      await user.click(renameTrigger);

      // Click inside the input
      const editInput = getEditInput()!;
      await user.click(editInput);

      // Should not trigger session load
      expect(onLoadSession).not.toHaveBeenCalled();
    });
  });

  describe('custom session preview', () => {
    it('uses custom getSessionPreview function when provided', () => {
      const customPreview = vi.fn((session: ChatSession) => `Custom: ${session.name}`);
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, getSessionPreview: customPreview });
      render(<ChatHistorySidebar {...props} />);

      expect(customPreview).toHaveBeenCalledWith(sessions[0]);
      expect(screen.getByText('Custom: Session 1')).toBeInTheDocument();
    });

    it('displays "No messages yet" for sessions without user messages', () => {
      const sessions = [
        createMockChatSession({
          id: 'session-1',
          name: 'Empty Session',
          messages: [],
        }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByText('No messages yet')).toBeInTheDocument();
    });

    it('truncates long preview text with ellipsis', () => {
      const longMessage =
        'This is a very long message that should be truncated because it exceeds the maximum character limit for preview text';
      const sessions = [
        createMockChatSession({
          id: 'session-1',
          name: 'Long Message Session',
          messages: [
            createMockChatMessage({
              role: 'user',
              content: longMessage,
            }),
          ],
        }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      // Should show first 50 chars + ellipsis
      expect(screen.getByText(/This is a very long message that should be trunc\.\.\./)).toBeInTheDocument();
    });
  });

  describe('session actions menu', () => {
    it('renders session actions for each session', () => {
      const sessions = createMockSessions(2);
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      expect(screen.getByTestId('session-actions-session-1')).toBeInTheDocument();
      expect(screen.getByTestId('session-actions-session-2')).toBeInTheDocument();
    });

    it('calls onManageTags when tags action is clicked', async () => {
      const user = userEvent.setup();
      const onManageTags = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onManageTags });
      render(<ChatHistorySidebar {...props} />);

      await user.click(screen.getByTestId('tags-trigger-session-1'));

      expect(onManageTags).toHaveBeenCalledWith('session-1');
    });

    it('calls onSetCategory when category action is clicked', async () => {
      const user = userEvent.setup();
      const onSetCategory = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onSetCategory });
      render(<ChatHistorySidebar {...props} />);

      await user.click(screen.getByTestId('category-trigger-session-1'));

      expect(onSetCategory).toHaveBeenCalledWith('session-1');
    });

    it('calls onShare when share action is clicked', async () => {
      const user = userEvent.setup();
      const onShare = vi.fn();
      const sessions = createMockSessions(1);
      const props = createDefaultProps({ sessions, onShare });
      render(<ChatHistorySidebar {...props} />);

      await user.click(screen.getByTestId('share-trigger-session-1'));

      expect(onShare).toHaveBeenCalledWith('session-1');
    });
  });

  describe('date display', () => {
    it('displays formatted date for each session', () => {
      const testDate = new Date('2024-06-15');
      const sessions = [
        createMockChatSession({
          id: 'session-1',
          name: 'Test Session',
          updatedAt: testDate,
        }),
      ];
      const props = createDefaultProps({ sessions });
      render(<ChatHistorySidebar {...props} />);

      // The date should be formatted according to locale
      expect(screen.getByText(testDate.toLocaleDateString())).toBeInTheDocument();
    });
  });
});
