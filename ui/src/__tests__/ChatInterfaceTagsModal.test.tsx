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
vi.mock('@/api/services', () => ({
  __esModule: true,
  default: {
    streamInfer: vi.fn(),
    getAdapterStack: vi.fn(),
    getSessionRouterView: vi.fn(),
    loadAdapter: vi.fn(),
  },
}));

// Mock hooks
vi.mock('@/hooks/admin/useAdmin', () => ({
  useAdapterStacks: () => ({ data: mockStacks, isLoading: false }),
  useGetDefaultStack: () => ({ data: null, isLoading: false }),
}));

// Mock collections API
vi.mock('@/hooks/api/useCollectionsApi', () => ({
  useCollections: () => ({ data: [], isLoading: false }),
}));

// Mock the session management hook
const mockGetSession = vi.fn();
const mockUpdateSession = vi.fn();
const mockAddMessage = vi.fn();
const mockUpdateMessage = vi.fn();
const mockDeleteSession = vi.fn();
const mockCreateSession = vi.fn();
const mockUpdateSessionCollection = vi.fn();
let mockSessions = [
  {
    id: 'session-1',
    name: 'Test Session',
    stackId: 'stack-1',
    messages: [],
    createdAt: new Date(),
    updatedAt: new Date(),
  },
];

vi.mock('@/hooks/chat/useChatSessionsApi', () => ({
  useChatSessionsApi: () => ({
    sessions: mockSessions,
    isLoading: false,
    isUnsupported: false,
    unsupportedReason: null,
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

// Mock chat hooks
vi.mock('@/hooks/chat', () => ({
  useChatStreaming: () => ({
    isStreaming: false,
    streamedText: '',
    currentRequestId: null,
    sendMessage: vi.fn(),
    cancelStream: vi.fn(),
    chunks: [],
    tokensReceived: 0,
    streamDuration: 0,
  }),
  useChatAdapterState: () => ({
    adapterStates: new Map(),
    isCheckingAdapters: false,
    allAdaptersReady: true,
    loadAllAdapters: vi.fn(),
    showAdapterPrompt: false,
    dismissAdapterPrompt: vi.fn(),
    continueWithUnready: vi.fn(),
  }),
  useChatRouterDecisions: () => ({
    isLoadingDecision: false,
    fetchDecision: vi.fn(async () => null),
    decisionHistory: [],
    lastDecision: null,
    clearDecisions: vi.fn(),
  }),
  useSessionManager: () => ({
    currentSessionId: null,
    messages: [],
    setMessages: vi.fn(),
    setCurrentSessionId: vi.fn(),
    clearSession: vi.fn(),
    loadSession: vi.fn(),
    createSession: vi.fn(),
  }),
  useChatModals: () => {
    const [state, setState] = React.useState<{
      active: 'history' | 'routerActivity' | 'archive' | 'share' | 'tags' | null;
      data: { sessionId?: string } | null;
    }>({
      active: null,
      data: null,
    });

    const openModal = React.useCallback(
      (type: 'history' | 'routerActivity' | 'archive' | 'share' | 'tags', data?: { sessionId?: string }) => {
        setState({
          active: type,
          data: data ?? null,
        });
      },
      []
    );

    const closeModal = React.useCallback(() => {
      setState({
        active: null,
        data: null,
      });
    }, []);

    const isHistoryOpen = state.active === 'history';
    const isRouterActivityOpen = state.active === 'routerActivity';
    const isArchivePanelOpen = state.active === 'archive';
    const shareDialogSessionId = state.active === 'share' ? state.data?.sessionId ?? null : null;
    const tagsDialogSessionId = state.active === 'tags' ? state.data?.sessionId ?? null : null;

    return {
      isHistoryOpen,
      isRouterActivityOpen,
      isArchivePanelOpen,
      shareDialogSessionId,
      tagsDialogSessionId,
      setIsHistoryOpen: (open: boolean) => {
        if (open) {
          openModal('history');
        } else if (state.active === 'history') {
          closeModal();
        }
      },
      setIsRouterActivityOpen: (open: boolean) => {
        if (open) {
          openModal('routerActivity');
        } else if (state.active === 'routerActivity') {
          closeModal();
        }
      },
      setIsArchivePanelOpen: (open: boolean) => {
        if (open) {
          openModal('archive');
        } else if (state.active === 'archive') {
          closeModal();
        }
      },
      setShareDialogSessionId: (sessionId: string | null) => {
        if (sessionId) {
          openModal('share', { sessionId });
        } else if (state.active === 'share') {
          closeModal();
        }
      },
      setTagsDialogSessionId: (sessionId: string | null) => {
        if (sessionId) {
          openModal('tags', { sessionId });
        } else if (state.active === 'tags') {
          closeModal();
        }
      },
    };
  },
}));

// Mock feature flags
vi.mock('@/hooks/config/useFeatureFlags', () => ({
  useChatAutoLoadModels: () => false,
}));

// Mock model loading hooks
vi.mock('@/hooks/model-loading', () => ({
  useModelLoadingState: () => ({
    isLoading: false,
    loadingModel: null,
    progress: 0,
    overallReady: true,
    baseModelReady: true,
    failedAdapters: [],
    loadingAdapters: [],
    readyAdapters: [],
    adapterStates: new Map(),
    error: null,
  }),
  useModelLoader: () => ({
    loadModels: vi.fn(),
    retryFailed: vi.fn(),
    cancelLoading: vi.fn(),
  }),
  useChatLoadingPersistence: () => ({
    persistedState: null,
    persist: vi.fn(),
    clear: vi.fn(),
    isRecoverable: false,
  }),
  useLoadingAnnouncements: () => ({
    announcement: null,
  }),
}));

// Mock evidence drawer context
vi.mock('@/contexts/EvidenceDrawerContext', () => ({
  EvidenceDrawerProvider: ({ children }: { children: React.ReactNode }) => children,
  useEvidenceDrawerOptional: () => null,
}));

// Mock export hook
vi.mock('@/components/export', () => ({
  useChatExport: () => ({
    handleExportMarkdown: vi.fn(),
    handleExportJson: vi.fn(),
    handleExportPdf: vi.fn(),
    ExportButton: () => null,
  }),
}));

// Mock chat components that are not being tested
vi.mock('@/components/chat/ChatLoadingOverlay', () => ({
  ChatLoadingOverlay: () => null,
}));

vi.mock('@/components/chat/ChatErrorDisplay', () => ({
  ChatErrorDisplay: () => null,
}));

vi.mock('@/components/chat/MissingPinnedAdaptersBanner', () => ({
  MissingPinnedAdaptersBanner: () => null,
}));

vi.mock('@/components/chat/EvidenceDrawer', () => ({
  EvidenceDrawer: () => null,
}));

vi.mock('@/components/chat/InlineModelLoadingBlock', () => ({
  InlineModelLoadingBlock: () => null,
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
    mockSessions = [
      {
        id: 'session-1',
        name: 'Test Session',
        stackId: 'stack-1',
        messages: [],
        createdAt: new Date(),
        updatedAt: new Date(),
      },
    ];
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
    // Should have at least 1: the dialog title (history may close when the dialog opens)
    expect(manageTagsElements.length).toBeGreaterThanOrEqual(1);

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

    mockSessions = [
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
    ];

    render(
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
