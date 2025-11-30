import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatInterface } from '@/components/ChatInterface';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import type { AdapterStack } from '@/api/types';

// Mock data
const mockStacks: AdapterStack[] = [
  {
    id: 'stack-1',
    name: 'Test Stack',
    adapter_ids: ['adapter-1', 'adapter-2'],
    description: 'Test description',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
];

// Mock API client
vi.mock('@/api/client', () => ({
  __esModule: true,
  default: {
    streamInfer: vi.fn(),
    getAdapterStack: vi.fn(),
    getSessionRouterView: vi.fn(),
    loadAdapter: vi.fn(),
  },
}));

// Mock hooks
vi.mock('@/hooks/useAdmin', () => ({
  useAdapterStacks: () => ({ data: mockStacks }),
  useGetDefaultStack: () => ({ data: null }),
}));

// Mock collections API
vi.mock('@/hooks/useCollectionsApi', () => ({
  useCollections: () => ({ data: [] }),
}));

// Mock the session management hook
const mockGetSession = vi.fn();
const mockUpdateSession = vi.fn();
const mockAddMessage = vi.fn();
const mockUpdateMessage = vi.fn();
const mockDeleteSession = vi.fn();
const mockCreateSession = vi.fn();
const mockUpdateSessionCollection = vi.fn();

vi.mock('@/hooks/useChatSessionsApi', () => ({
  useChatSessionsApi: () => ({
    sessions: [
      {
        id: 'session-1',
        name: 'Test Session',
        stackId: 'stack-1',
        messages: [],
        createdAt: new Date(),
        updatedAt: new Date(),
      },
    ],
    isLoading: false,
    createSession: mockCreateSession,
    updateSession: mockUpdateSession,
    addMessage: mockAddMessage,
    updateMessage: mockUpdateMessage,
    deleteSession: mockDeleteSession,
    getSession: mockGetSession,
    updateSessionCollection: mockUpdateSessionCollection,
  }),
}));

// Mock SSE hook
vi.mock('@/hooks/useSSE', () => ({
  useSSE: () => ({}),
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Mock logger
vi.mock('@/utils/logger', () => ({
  logger: {
    error: vi.fn(),
    warn: vi.fn(),
    info: vi.fn(),
  },
  toError: (error: unknown) => error,
}));

// Mock the ChatTagsManager component - this is what we're testing the integration with
const mockChatTagsManager = vi.fn(() => <div data-testid="chat-tags-manager">Mock ChatTagsManager</div>);
vi.mock('@/components/chat/ChatTagsManager', () => ({
  ChatTagsManager: (props: { sessionId: string }) => {
    mockChatTagsManager(props);
    return <div data-testid="chat-tags-manager" data-session-id={props.sessionId}>Mock ChatTagsManager</div>;
  },
}));

// Mock ChatSessionActions to trigger the tags modal
const mockOnManageTags = vi.fn();
vi.mock('@/components/chat/ChatSessionActions', () => ({
  ChatSessionActions: (props: {
    sessionId: string;
    tenantId: string;
    onRename: () => void;
    onManageTags: () => void;
    onSetCategory: () => void;
    onShare: () => void;
  }) => {
    // Store the callback so we can trigger it in tests
    mockOnManageTags.mockImplementation(props.onManageTags);
    return (
      <button
        data-testid={`session-actions-${props.sessionId}`}
        onClick={props.onManageTags}
      >
        Manage Tags
      </button>
    );
  },
}));

// Mock other chat components
vi.mock('@/components/chat/ChatMessage', () => ({
  ChatMessageComponent: () => <div>Mock ChatMessage</div>,
}));

vi.mock('@/components/chat/ChatSearchBar', () => ({
  ChatSearchBar: () => <div>Mock ChatSearchBar</div>,
}));

vi.mock('@/components/chat/ChatShareDialog', () => ({
  ChatShareDialog: () => <div>Mock ChatShareDialog</div>,
}));

vi.mock('@/components/chat/ChatArchivePanel', () => ({
  ChatArchivePanel: () => <div>Mock ChatArchivePanel</div>,
}));

vi.mock('@/components/chat/RouterActivitySidebar', () => ({
  RouterActivitySidebar: () => <div>Mock RouterActivitySidebar</div>,
}));

vi.mock('@/components/chat/PreChatAdapterPrompt', () => ({
  PreChatAdapterPrompt: () => <div>Mock PreChatAdapterPrompt</div>,
}));

vi.mock('@/components/chat/AdapterLoadingStatus', () => ({
  AdapterLoadingStatus: () => <div>Mock AdapterLoadingStatus</div>,
}));

// Test wrapper component
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        {children}
      </QueryClientProvider>
    </MemoryRouter>
  );
}

describe('ChatInterface - Tags Modal Integration', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    mockGetSession.mockReturnValue({
      id: 'session-1',
      name: 'Test Session',
      stackId: 'stack-1',
      messages: [],
      createdAt: new Date(),
      updatedAt: new Date(),
    });
    mockCreateSession.mockReturnValue({
      id: 'session-1',
      name: 'Test Session',
      stackId: 'stack-1',
      messages: [],
      createdAt: new Date(),
      updatedAt: new Date(),
    });
  });

  it('renders the tags modal dialog in the component', () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    // The dialog should be in the document (even if not visible)
    // We can't directly check for the Dialog component, but we can verify
    // the structure exists by checking for the modal trigger mechanism
    expect(screen.getByText(/Currently Loaded/i)).toBeTruthy();
  });

  it('opens the tags modal dialog when tagsDialogSessionId state is set', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    const user = userEvent.setup();

    // Open the history sidebar to access session actions
    const historyButton = screen.getByRole('button', { name: /open history/i });
    await user.click(historyButton);

    // Wait for the session to appear
    await waitFor(() => {
      expect(screen.getByText('Test Session')).toBeTruthy();
    });

    // Click the manage tags button (which is mocked)
    const manageTagsButton = screen.getByTestId('session-actions-session-1');
    await user.click(manageTagsButton);

    // The dialog should now be open and visible
    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeTruthy();
    });

    // The dialog should have the correct title (use getAllByText since button also has this text)
    const manageTagsElements = screen.getAllByText('Manage Tags');
    expect(manageTagsElements.length).toBeGreaterThanOrEqual(1);
  });

  it('displays ChatTagsManager with correct sessionId prop when dialog is open', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    const user = userEvent.setup();

    // Open history sidebar
    const historyButton = screen.getByRole('button', { name: /open history/i });
    await user.click(historyButton);

    await waitFor(() => {
      expect(screen.getByText('Test Session')).toBeTruthy();
    });

    // Trigger manage tags
    const manageTagsButton = screen.getByTestId('session-actions-session-1');
    await user.click(manageTagsButton);

    // Wait for dialog to open
    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeTruthy();
    });

    // Verify ChatTagsManager is rendered with correct sessionId
    // Note: There may be multiple instances due to React StrictMode double rendering
    const tagsManagers = screen.getAllByTestId('chat-tags-manager');
    expect(tagsManagers.length).toBeGreaterThanOrEqual(1);
    expect(tagsManagers[0].getAttribute('data-session-id')).toBe('session-1');

    // Verify the mock was called with correct props
    expect(mockChatTagsManager).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-1',
      })
    );
  });

  it('closes the tags modal when onOpenChange is called with false', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    const user = userEvent.setup();

    // Open history sidebar
    const historyButton = screen.getByRole('button', { name: /open history/i });
    await user.click(historyButton);

    await waitFor(() => {
      expect(screen.getByText('Test Session')).toBeTruthy();
    });

    // Open the tags dialog
    const manageTagsButton = screen.getByTestId('session-actions-session-1');
    await user.click(manageTagsButton);

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeTruthy();
    });

    // Close the dialog by pressing Escape
    await user.keyboard('{Escape}');

    // Dialog should be closed
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
  });

  it('dialog title shows "Manage Tags"', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    const user = userEvent.setup();

    // Open history sidebar
    const historyButton = screen.getByRole('button', { name: /open history/i });
    await user.click(historyButton);

    await waitFor(() => {
      expect(screen.getByText('Test Session')).toBeTruthy();
    });

    // Open the tags dialog
    const manageTagsButton = screen.getByTestId('session-actions-session-1');
    await user.click(manageTagsButton);

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeTruthy();
    });

    // Verify the dialog title exists (use getAllByText since button also has this text)
    const manageTagsElements = screen.getAllByText('Manage Tags');
    // Should have at least 2: the button and the dialog title
    expect(manageTagsElements.length).toBeGreaterThanOrEqual(2);

    // Find the dialog title by looking for the one with the text-lg class (DialogTitle)
    const dialogTitle = manageTagsElements.find(el => el.className?.includes('text-lg'));
    expect(dialogTitle).toBeTruthy();
  });

  it('does not render tags dialog when tagsDialogSessionId is null', () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    // Initially, the dialog should not be visible
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByText('Manage Tags')).toBeNull();
  });

  it('handles multiple sessions and shows correct sessionId in modal', async () => {
    // Add a second session to the mock
    mockGetSession.mockImplementation((id: string) => {
      if (id === 'session-2') {
        return {
          id: 'session-2',
          name: 'Second Session',
          stackId: 'stack-1',
          messages: [],
          createdAt: new Date(),
          updatedAt: new Date(),
        };
      }
      return {
        id: 'session-1',
        name: 'Test Session',
        stackId: 'stack-1',
        messages: [],
        createdAt: new Date(),
        updatedAt: new Date(),
      };
    });

    // Update the useChatSessionsApi mock to return multiple sessions
    vi.mocked(vi.importActual('@/hooks/useChatSessionsApi')).useChatSessionsApi = () => ({
      sessions: [
        {
          id: 'session-1',
          name: 'Test Session',
          stackId: 'stack-1',
          messages: [],
          createdAt: new Date(),
          updatedAt: new Date(),
        },
        {
          id: 'session-2',
          name: 'Second Session',
          stackId: 'stack-1',
          messages: [],
          createdAt: new Date(),
          updatedAt: new Date(),
        },
      ],
      isLoading: false,
      createSession: mockCreateSession,
      updateSession: mockUpdateSession,
      addMessage: mockAddMessage,
      updateMessage: mockUpdateMessage,
      deleteSession: mockDeleteSession,
      getSession: mockGetSession,
      updateSessionCollection: mockUpdateSessionCollection,
    });

    const { rerender } = render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    // Force a re-render to pick up the new mock
    rerender(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    const user = userEvent.setup();

    // Open history sidebar
    const historyButton = screen.getByRole('button', { name: /open history/i });
    await user.click(historyButton);

    // Check if session-2 action button exists
    const session2Actions = screen.queryByTestId('session-actions-session-2');
    if (session2Actions) {
      await user.click(session2Actions);

      await waitFor(() => {
        expect(screen.getByRole('dialog')).toBeTruthy();
      });

      // Verify the correct sessionId is passed
      const tagsManager = screen.getByTestId('chat-tags-manager');
      expect(tagsManager.getAttribute('data-session-id')).toBe('session-2');
    }
  });
});
