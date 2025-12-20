/**
 * ChatInterface Test - Mock Factory Example
 *
 * This file demonstrates how to use the centralized mock factories
 * instead of the manual mock setup pattern.
 *
 * BEFORE (original ChatInterface.test.tsx):
 * - ~200 lines of vi.mock() calls at top of file
 * - Manually defined all hook return values
 * - No type safety on mock values
 *
 * AFTER (this file):
 * - Uses createMutableAuthState() and factory functions
 * - Type-safe mock factories with autocomplete
 * - Easy overrides for specific test cases
 *
 * KEY INSIGHT: vi.mock() calls are hoisted to the top of the module,
 * so we use mutable state objects that vi.mock() references, allowing
 * us to update the mock values in beforeEach or individual tests.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatInterface } from '@/components/ChatInterface';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { FeatureProviders } from '@/providers/FeatureProviders';
import type { AdapterStack } from '@/api/types';

// Import mock factories
import {
  createMutableAuthState,
  createUseAuthMock,
} from '@/test/mocks/appliers/auth';
import {
  createUseModelLoadingStateMock,
  createUseModelLoaderMock,
  createUseChatLoadingPersistenceMock,
  createUseLoadingAnnouncementsMock,
} from '@/test/mocks/hooks/model-loading';
import {
  createUseChatSessionsApiMock,
  createUseChatStreamingMock,
  createUseChatAdapterStateMock,
  createUseChatRouterDecisionsMock,
  createUseSessionManagerMock,
  createUseChatModalsMock,
} from '@/test/mocks/hooks/chat';

// ============================================================================
// Test-specific data
// ============================================================================

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
];

// ============================================================================
// Create mutable mock states (these hold the actual mock values)
// ============================================================================

// Auth mock - can be updated in beforeEach or individual tests
const authState = createMutableAuthState({ user: { tenant_id: 'test-tenant' } });

// Model loading mocks
const modelLoadingState = createUseModelLoadingStateMock({ isReady: true });
const modelLoader = createUseModelLoaderMock();
const loadingPersistence = createUseChatLoadingPersistenceMock();
const loadingAnnouncements = createUseLoadingAnnouncementsMock();

// Chat mocks
const chatSessionsApi = createUseChatSessionsApiMock();
const chatStreaming = createUseChatStreamingMock();
const chatAdapterState = createUseChatAdapterStateMock();
const chatRouterDecisions = createUseChatRouterDecisionsMock();
const sessionManager = createUseSessionManagerMock();
const chatModals = createUseChatModalsMock();

// API mocks
const mockStreamInfer = vi.fn();
const mockGetAdapterStack = vi.fn();
const mockGetSessionRouterView = vi.fn();
const mockListUserTenants = vi.fn();
const mockGetUserProfile = vi.fn();
const mockRefreshSession = vi.fn();

// ============================================================================
// vi.mock() calls - hoisted to module scope by Vitest
// Reference the mutable state objects so updates take effect
// ============================================================================

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

vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => authState.current,
  useResize: () => ({
    getLayout: vi.fn(() => null),
    setLayout: vi.fn(),
  }),
  TENANT_SELECTION_REQUIRED_KEY: 'aos-tenant-selection-required',
}));

vi.mock('@/hooks/model-loading', () => ({
  useModelLoadingState: () => modelLoadingState,
  useModelLoader: () => modelLoader,
  useChatLoadingPersistence: () => loadingPersistence,
  useLoadingAnnouncements: () => loadingAnnouncements,
}));

vi.mock('@/hooks/chat/useChatSessionsApi', () => ({
  useChatSessionsApi: () => chatSessionsApi,
}));

vi.mock('@/hooks/chat', () => ({
  useChatStreaming: () => chatStreaming,
  useChatAdapterState: () => chatAdapterState,
  useChatRouterDecisions: () => chatRouterDecisions,
  useSessionManager: () => sessionManager,
  useChatModals: () => chatModals,
}));

// Test-specific hooks that need per-test overrides via hoisted pattern
const mockUseAdapterStacks = vi.hoisted(() => vi.fn());
const mockUseGetDefaultStack = vi.hoisted(() => vi.fn());

vi.mock('@/hooks/admin/useAdmin', () => ({
  useAdapterStacks: () => mockUseAdapterStacks(),
  useGetDefaultStack: (tenantId: string) => mockUseGetDefaultStack(tenantId),
}));

vi.mock('@/hooks/api/useCollectionsApi', () => ({
  useCollections: () => ({ data: [], isLoading: false }),
}));

vi.mock('@/hooks/config/useFeatureFlags', () => ({
  useChatAutoLoadModels: () => false,
}));

vi.mock('@/components/export', () => ({
  useChatExport: () => ({
    handleExportMarkdown: vi.fn(),
    handleExportJson: vi.fn(),
    handleExportPdf: vi.fn(),
    ExportButton: () => null,
  }),
}));

vi.mock('@/components/chat/ChatTagsManager', () => ({
  ChatTagsManager: () => null,
}));

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock('@/utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn() },
  toError: (error: unknown) => error,
}));

// ============================================================================
// Test wrapper
// ============================================================================

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

// ============================================================================
// Tests
// ============================================================================

describe('ChatInterface - Mock Factory Example', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    sessionStorage.clear();

    // Reset auth state to defaults
    authState.reset();

    // Setup test-specific mock data
    mockUseAdapterStacks.mockReturnValue({ data: mockStacks });
    mockUseGetDefaultStack.mockReturnValue({ data: null });

    // Configure API mock for tenant list
    mockListUserTenants.mockResolvedValue([
      { id: 'test-tenant', name: 'Test Tenant' },
      { id: 'default', name: 'Default Tenant' },
    ]);
  });

  it('displays "No stack selected" when selectedStack is null', () => {
    mockUseAdapterStacks.mockReturnValue({ data: [] });

    render(
      <TestWrapper>
        <ChatInterface selectedTenant="test-tenant" />
      </TestWrapper>
    );

    expect(screen.getByText('No stack selected')).toBeTruthy();
  });

  it('shows default stack badge when stack matches defaultStack', async () => {
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

  it('demonstrates accessing mock methods for assertions', async () => {
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

    // Type and send message
    const input = screen.getByPlaceholderText(/Type your message/);
    await user.type(input, 'Hello world');

    const sendButton = screen.getByRole('button', { name: /send message/i });
    await user.click(sendButton);

    // Access mock through the chatStreaming object defined at module scope
    await waitFor(() => {
      expect(chatStreaming.sendMessage).toHaveBeenCalled();
    });
  });

  it('demonstrates updating auth state per-test', () => {
    // Update auth to viewer role for this specific test
    authState.update({ user: { role: 'viewer', tenant_id: 'viewer-tenant' } });

    render(
      <TestWrapper>
        <ChatInterface selectedTenant="viewer-tenant" />
      </TestWrapper>
    );

    // Test runs with viewer auth context
    expect(screen.getByText('No stack selected')).toBeTruthy();
  });
});

/**
 * Summary of improvements:
 *
 * FACTORY FUNCTIONS USED:
 * - createMutableAuthState() - for auth with per-test updates
 * - createUseModelLoadingStateMock() - for model loading state
 * - createUseChatSessionsApiMock() - for chat sessions
 * - createUseChatStreamingMock() - for streaming state
 * - And more...
 *
 * BENEFITS:
 * 1. Type-safe mock values with autocomplete
 * 2. Sensible defaults (no need to specify every field)
 * 3. Easy per-test overrides via authState.update()
 * 4. Centralized maintenance of mock shapes
 * 5. Consistent mock patterns across test files
 *
 * STILL REQUIRED:
 * - vi.mock() calls at module scope (Vitest hoisting requirement)
 * - Test-specific data (mockStacks in this case)
 * - Hoisted functions for per-test overrides (mockUseAdapterStacks)
 */
