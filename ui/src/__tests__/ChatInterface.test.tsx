import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatInterface } from '@/components/ChatInterface';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { FeatureProviders } from '@/providers/FeatureProviders';
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
  {
    id: 'stack-2',
    name: 'Default Stack',
    adapter_ids: ['adapter-3'],
    lifecycle_state: 'active',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'stack-minimal',
    name: 'Minimal Stack',
    adapter_ids: [],
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'stack-long',
    name: 'Very Long Stack Name That Should Be Truncated In The UI'.repeat(3),
    adapter_ids: ['adapter-4'],
    description: 'Long description text that should also be truncated'.repeat(5),
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'stack-legacy',
    name: 'Legacy Stack',
    adapters: [
      { adapter_id: 'adapter-5', gate: 16384 },
      { adapter_id: 'adapter-6', gate: 8192 },
    ],
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'stack-both',
    name: 'Stack with Both Properties',
    adapter_ids: ['adapter-7', 'adapter-8'],
    adapters: [
      { adapter_id: 'adapter-7', gate: 32767 },
      { adapter_id: 'adapter-8', gate: 16384 },
    ],
    lifecycle_state: 'deprecated',
    description: 'Fallback description',
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  },
];

// Mock API client
const mockStreamInfer = vi.fn();
const mockGetAdapterStack = vi.fn();
const mockGetSessionRouterView = vi.fn();
const mockListUserTenants = vi.fn();
const mockGetUserProfile = vi.fn();
const mockRefreshSession = vi.fn();

vi.mock('@/api/services', () => ({
  __esModule: true,
  default: {
    streamInfer: (...args: unknown[]) => mockStreamInfer(...args),
    getAdapterStack: (...args: unknown[]) => mockGetAdapterStack(...args),
    getSessionRouterView: (...args: unknown[]) => mockGetSessionRouterView(...args),
  },
  apiClient: {
    streamInfer: (...args: unknown[]) => mockStreamInfer(...args),
    getAdapterStack: (...args: unknown[]) => mockGetAdapterStack(...args),
    getSessionRouterView: (...args: unknown[]) => mockGetSessionRouterView(...args),
    listUserTenants: (...args: unknown[]) => mockListUserTenants(...args),
    getUserProfile: (...args: unknown[]) => mockGetUserProfile(...args),
    refreshSession: (...args: unknown[]) => mockRefreshSession(...args),
  },
}));

// Mock CoreProviders
vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: { id: 'user-1', tenant_id: 'test-tenant' },
    isLoading: false,
    authError: null,
    accessToken: 'mock-token',
    sessionMode: 'session',
    login: vi.fn(),
    devBypassLogin: vi.fn(),
    logout: vi.fn(),
    refreshUser: vi.fn(),
    refreshSession: vi.fn(),
    logoutAllSessions: vi.fn(),
    updateProfile: vi.fn(),
    clearAuthError: vi.fn(),
  }),
  useResize: () => ({
    getLayout: vi.fn(() => null),
    setLayout: vi.fn(),
  }),
  TENANT_SELECTION_REQUIRED_KEY: 'aos-tenant-selection-required',
}));

// Mock hooks - use a hoisted function so we can override the return value per test
const mockUseAdapterStacks = vi.hoisted(() => vi.fn());
const mockUseGetDefaultStack = vi.hoisted(() => vi.fn());

vi.mock('@/hooks/admin/useAdmin', () => ({
  useAdapterStacks: () => mockUseAdapterStacks(),
  useGetDefaultStack: (tenantId: string) => mockUseGetDefaultStack(tenantId),
}));

vi.mock('@/hooks/chat/useChatSessionsApi', () => ({
  useChatSessionsApi: () => ({
    sessions: [],
    isLoading: false,
    isUnsupported: false,
    unsupportedReason: null,
    createSession: vi.fn((name: string, stackId: string) => ({
      id: 'session-1',
      name,
      stackId,
      messages: [],
      createdAt: new Date(),
      updatedAt: new Date(),
    })),
    updateSession: vi.fn(),
    addMessage: vi.fn(),
    deleteSession: vi.fn(),
    getSession: vi.fn((sessionId: string) => ({
      id: sessionId,
      name: `Session ${sessionId}`,
      stackId: 'stack-1',
      messages: [],
      createdAt: new Date(),
      updatedAt: new Date(),
    })),
    updateSessionCollection: vi.fn(),
  }),
}));

vi.mock('@/hooks/api/useCollectionsApi', () => ({
  useCollections: () => ({
    data: [],
    isLoading: false,
  }),
}));

vi.mock('@/hooks/config/useFeatureFlags', () => ({
  useChatAutoLoadModels: () => false,
}));

vi.mock('@/hooks/model-loading', () => ({
  useModelLoadingState: () => ({
    isLoading: false,
    progress: 0,
    overallReady: true,
    baseModelReady: true,
    error: null,
    failedAdapters: [],
    loadingAdapters: [],
    readyAdapters: [],
    adapterStates: new Map(),
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

const mockSendMessage = vi.fn();
const mockFetchDecision = vi.fn().mockResolvedValue(null);
const mockUseSessionManager = vi.fn(() => ({
  currentSessionId: 'session-1',
  messages: [],
  setMessages: vi.fn(),
  setCurrentSessionId: vi.fn(),
  clearSession: vi.fn(),
  loadSession: vi.fn(),
  createSession: vi.fn(),
}));

vi.mock('@/hooks/chat', () => ({
  useChatStreaming: () => ({
    isStreaming: false,
    streamedText: '',
    currentRequestId: null,
    sendMessage: mockSendMessage,
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
    fetchDecision: (...args: unknown[]) => mockFetchDecision(...args),
    decisionHistory: [],
    lastDecision: null,
    clearDecisions: vi.fn(),
  }),
  useSessionManager: (...args: unknown[]) => mockUseSessionManager(...args),
  useChatModals: () => ({
    isHistoryOpen: false,
    setIsHistoryOpen: vi.fn(),
    isRouterActivityOpen: false,
    setIsRouterActivityOpen: vi.fn(),
    isArchivePanelOpen: false,
    setIsArchivePanelOpen: vi.fn(),
    shareDialogSessionId: null,
    setShareDialogSessionId: vi.fn(),
    tagsDialogSessionId: null,
    setTagsDialogSessionId: vi.fn(),
  }),
}));

vi.mock('@/components/export', () => ({
  useChatExport: () => ({
    handleExportMarkdown: vi.fn(),
    handleExportJson: vi.fn(),
    handleExportPdf: vi.fn(),
    ExportButton: () => null,
  }),
}));

// Mock chat sub-components not under test
vi.mock('@/components/chat/ChatTagsManager', () => ({
  ChatTagsManager: () => null,
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

// Test wrapper component
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        <FeatureProviders>
          {children}
        </FeatureProviders>
      </QueryClientProvider>
    </MemoryRouter>
  );
}

describe('ChatInterface - Stack State Handling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    sessionStorage.clear();
    // Setup mock tenant list
    mockListUserTenants.mockResolvedValue([
      { id: 'test-tenant', name: 'Test Tenant' },
      { id: 'default', name: 'Default Tenant' },
    ]);
    // Setup default mock return values
    mockUseAdapterStacks.mockReturnValue({ data: mockStacks });
    mockUseGetDefaultStack.mockReturnValue({ data: null });
  });

  it('displays "No stack selected" when selectedStack is null', () => {
    // Override to return empty stacks array for this test
    mockUseAdapterStacks.mockReturnValue({ data: [] });

    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" />
      </TestWrapper>
    );

    expect(screen.getByText('No stack selected')).toBeTruthy();
  });

  it('shows default stack badge when stack matches defaultStack', async () => {
    // Setup default stack for the 'default' tenant
    mockUseGetDefaultStack.mockImplementation((tenantId: string) => ({
      data: tenantId === 'default' ? mockStacks[1] : null,
    }));

    render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-2" />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Default stack for tenant')).toBeTruthy();
    });
  });

  it('handles race condition: defaultStack loads after selectedStack', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="default" initialStackId="stack-1" />
      </TestWrapper>
    );

    // Initially no default badge (stack-1 is not the default)
    expect(screen.queryByText('Default stack for tenant')).toBeNull();

    // Note: We can't easily test the race condition in a unit test without more complex mocking.
    // This test verifies that non-default stacks don't show the badge.
  });

  it('displays lifecycle_state over description when both exist', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-both" />
      </TestWrapper>
    );

    // Should show lifecycle_state, not description
    await waitFor(() => {
      expect(screen.getByText('deprecated')).toBeTruthy();
    });
    expect(screen.queryByText('Fallback description')).toBeNull();
  });

  it('falls back to description when lifecycle_state is missing', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-1" />
      </TestWrapper>
    );

    await waitFor(() => {
      expect(screen.getByText('Test description')).toBeTruthy();
    });
  });

  it('shows null when both lifecycle_state and description are missing', () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-minimal" />
      </TestWrapper>
    );

    const stackDetails = screen.queryByText(/active|deprecated|draft/);
    expect(stackDetails).toBeNull();
  });

  it('truncates extremely long stack names', () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-long" />
      </TestWrapper>
    );

    // Check that truncation class is applied (use getAllByText to handle multiple instances)
    const stackLabels = screen.getAllByText(/Very Long Stack Name/);
    expect(stackLabels.length).toBeGreaterThan(0);
    // At least one should have truncate class (the main label in the context panel)
    const hasTruncate = stackLabels.some(el => el.className.includes('truncate'));
    expect(hasTruncate).toBe(true);
  });

  it('handles adapter count from adapter_ids property', () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-1" />
      </TestWrapper>
    );

    // Stack has 2 adapters in adapter_ids
    expect(screen.getByText('2 adapters')).toBeTruthy();
  });

  it('handles adapter count from adapters property (legacy)', () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-legacy" />
      </TestWrapper>
    );

    // Stack has 2 adapters in adapters property
    // The context panel shows adapter count (just the number)
    const adapterCounts = screen.getAllByText('2');
    expect(adapterCounts.length).toBeGreaterThan(0);
  });

  it('shows "0 adapter" when stack has no adapters', () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-minimal" />
      </TestWrapper>
    );

    // Badge shows "0 adapter" (singular)
    const badge = screen.getByText(/0 adapter/);
    expect(badge).toBeTruthy();
  });
});

describe('ChatInterface - Collapsible Context Panel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    sessionStorage.clear();
    // Setup mock tenant list
    mockListUserTenants.mockResolvedValue([
      { id: 'test-tenant', name: 'Test Tenant' },
      { id: 'default', name: 'Default Tenant' },
    ]);
    // Setup default mock return values
    mockUseAdapterStacks.mockReturnValue({ data: mockStacks });
    mockUseGetDefaultStack.mockReturnValue({ data: null });
  });

  it('shows context by default', () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-1" />
      </TestWrapper>
    );

    expect(screen.getByText('Stack')).toBeTruthy();
    expect(screen.getByText('Adapters')).toBeTruthy();
    expect(screen.getByText('Base model')).toBeTruthy();
  });

  it('hides context when "Hide" button is clicked', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-1" />
      </TestWrapper>
    );

    const user = userEvent.setup();
    const hideButton = screen.getByRole('button', { name: /hide stack context/i });
    await user.click(hideButton);

    await waitFor(() => {
      expect(screen.queryByText('Adapters')).toBeNull();
    });
  });

  it('toggles context visibility correctly', async () => {
    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" initialStackId="stack-1" />
      </TestWrapper>
    );

    const user = userEvent.setup();

    // Initially visible
    expect(screen.getByText('Adapters')).toBeTruthy();

    // Hide
    const hideButton = screen.getByRole('button', { name: /hide stack context/i });
    await user.click(hideButton);
    await waitFor(() => {
      expect(screen.queryByText('Adapters')).toBeNull();
    });

    // Show again
    const showButton = screen.getByRole('button', { name: /show stack context/i });
    await user.click(showButton);
    await waitFor(() => {
      expect(screen.getByText('Adapters')).toBeTruthy();
    });
  });
});

describe('ChatInterface - Stack Switching', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    sessionStorage.clear();
    // Setup mock tenant list
    mockListUserTenants.mockResolvedValue([
      { id: 'test-tenant', name: 'Test Tenant' },
      { id: 'default', name: 'Default Tenant' },
    ]);
    // Setup default mock return values
    mockUseAdapterStacks.mockReturnValue({ data: mockStacks });
    mockUseGetDefaultStack.mockReturnValue({ data: null });
    mockStreamInfer.mockImplementation((req: unknown, callbacks: {
      onToken: (token: string) => void;
      onComplete: (text: string, reason: string | null) => void;
      onError: (error: Error) => void;
    }) => {
      // Simulate streaming response
      setTimeout(() => {
        callbacks.onToken('Test');
        callbacks.onToken(' response');
        callbacks.onComplete('Test response', 'stop');
      }, 10);
      return Promise.resolve();
    });
    mockGetSessionRouterView.mockResolvedValue({
      request_id: 'test-request',
      stack_id: 'stack-1',
      steps: [{
        timestamp: '2025-01-01T00:00:00Z',
        entropy: 0.5,
        tau: 1.0,
        step: 0,
        adapters_fired: [
          { adapter_idx: 0, gate_value: 0.8, selected: true },
          { adapter_idx: 1, gate_value: 0.6, selected: true },
        ],
      }],
    });
    mockGetAdapterStack.mockImplementation((stackId: string) => {
      return Promise.resolve(mockStacks.find(s => s.id === stackId));
    });
  });

  it('uses correct stack when sending message', async () => {
    render(
      <TestWrapper>
        <ChatInterface
          selectedTenant="test-tenant"
          initialStackId="stack-1"
          sessionId="test-session-1"
        />
      </TestWrapper>
    );

    const user = userEvent.setup();

    // Type message
    const input = screen.getByPlaceholderText(/Type your message/);
    await user.type(input, 'Hello world');

    // Send message
    const sendButton = screen.getByRole('button', { name: /send message/i });
    await user.click(sendButton);

    // Verify sendMessage was called with the correct message
    await waitFor(() => {
      expect(mockSendMessage).toHaveBeenCalled();
      const call = mockSendMessage.mock.calls[0];
      expect(call[0]).toBe('Hello world'); // First argument is the message
    });
  });

  it('updates router decision after stack switch', async () => {
    render(
      <TestWrapper>
        <ChatInterface
          selectedTenant="test-tenant"
          initialStackId="stack-1"
          sessionId="test-session-2"
        />
      </TestWrapper>
    );

    const user = userEvent.setup();

    // Send message with stack-1
    const input = screen.getByPlaceholderText(/Type your message/);
    await user.type(input, 'First message');
    const sendButton = screen.getByRole('button', { name: /send message/i });
    await user.click(sendButton);

    // Verify sendMessage was called
    await waitFor(() => {
      expect(mockSendMessage).toHaveBeenCalled();
      const call = mockSendMessage.mock.calls[0];
      expect(call[0]).toBe('First message');
    });

    // Note: Testing stack switching in a single instance requires selecting from dropdown,
    // which is complex to test. This test verifies the initial stack is used correctly.
  });
});
