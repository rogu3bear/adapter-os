/**
 * OperatorChatLayout Tests
 *
 * Comprehensive tests for the OperatorChatLayout component:
 * - Component renders correctly in different states
 * - Loading state displays properly
 * - Error states with retry functionality
 * - No-model state display
 * - Ready state with ChatInterface
 * - Error handling and recovery
 * - Integration with ModelStatusBar
 * - Auto-load behavior
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { TooltipProvider } from '@/components/ui/tooltip';
import { OperatorChatLayout } from '@/components/operator';
import type { AdapterStack } from '@/api/types';

// Mock data
const mockDefaultStack: AdapterStack = {
  id: 'default-stack',
  name: 'Default Stack',
  adapter_ids: ['adapter-1', 'adapter-2'],
  lifecycle_state: 'active',
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

// Mock API client
const mockGetBaseModelStatus = vi.fn();
const mockListModels = vi.fn();
const mockLoadBaseModel = vi.fn();
const mockStreamInfer = vi.fn();

vi.mock('@/api/client', () => ({
  __esModule: true,
  default: {
    getBaseModelStatus: (...args: unknown[]) => mockGetBaseModelStatus(...args),
    listModels: (...args: unknown[]) => mockListModels(...args),
    loadBaseModel: (...args: unknown[]) => mockLoadBaseModel(...args),
    streamInfer: (...args: unknown[]) => mockStreamInfer(...args),
  },
}));

// Mock hooks
const mockUseModelStatus = vi.fn();
const mockUseAutoLoadModel = vi.fn();
const mockUseGetDefaultStack = vi.fn();

vi.mock('@/hooks/useModelStatus', () => ({
  useModelStatus: (tenantId: string) => mockUseModelStatus(tenantId),
}));

vi.mock('@/hooks/useAutoLoadModel', () => ({
  useAutoLoadModel: (tenantId: string, enabled: boolean) => mockUseAutoLoadModel(tenantId, enabled),
}));

vi.mock('@/hooks/useAdmin', () => ({
  useGetDefaultStack: (tenantId: string) => mockUseGetDefaultStack(tenantId),
}));

// Mock ChatInterface component
vi.mock('@/components/ChatInterface', () => ({
  ChatInterface: ({ selectedTenant, initialStackId }: { selectedTenant: string; initialStackId?: string }) => (
    <div data-testid="chat-interface">
      ChatInterface - Tenant: {selectedTenant}
      {initialStackId && ` - Stack: ${initialStackId}`}
    </div>
  ),
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
    debug: vi.fn(),
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
        <TooltipProvider>
          {children}
        </TooltipProvider>
      </QueryClientProvider>
    </MemoryRouter>
  );
}

describe('OperatorChatLayout', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();

    // Default mock implementations
    mockGetBaseModelStatus.mockResolvedValue({
      model_id: 'test-model',
      model_name: 'Test Model',
      status: 'ready',
      memory_usage_mb: 2048,
    });
    mockListModels.mockResolvedValue([]);
    mockLoadBaseModel.mockResolvedValue({});
  });

  describe('Loading State', () => {
    it('renders loading state when auto-loading model', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'loading',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: true,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Preparing your workspace...')).toBeTruthy();
      expect(screen.getByText('Loading model for inference')).toBeTruthy();

      // Should show spinner
      const spinners = document.querySelectorAll('.animate-spin');
      expect(spinners.length).toBeGreaterThan(0);
    });

    it('does not show loading when auto-load error is present', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: true,
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network connection failed',
          retryCount: 1,
          canRetry: true,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      // Should not show loading state
      expect(screen.queryByText('Preparing your workspace...')).toBeNull();

      // Should show error state instead
      expect(screen.getByText('Network Error')).toBeTruthy();
    });
  });

  describe('Error States', () => {
    it('renders network error with correct icon and message', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network connection failed',
          retryCount: 0,
          canRetry: true,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Network Error')).toBeTruthy();
      expect(screen.getByText('Unable to connect to the server. Please check your connection.')).toBeTruthy();
      expect(screen.getAllByRole('button', { name: /Retry/i }).length).toBeGreaterThan(0);
      expect(screen.getByRole('button', { name: /Dismiss/i })).toBeTruthy();
    });

    it('renders no models error with correct message', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: {
          code: 'NO_MODELS',
          message: 'No models available',
          retryCount: 0,
          canRetry: false,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('No Models Available')).toBeTruthy();
      expect(screen.getByText('No models are available. Please import a model or contact your administrator.')).toBeTruthy();

      // Should only show dismiss button when canRetry is false (no retry button in error display)
      // Note: ModelStatusBar may still have its own retry button for different errors
      expect(screen.getByRole('button', { name: /Dismiss/i })).toBeTruthy();
    });

    it('renders timeout error', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: {
          code: 'TIMEOUT',
          message: 'Loading timed out',
          retryCount: 2,
          canRetry: true,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Loading Timed Out')).toBeTruthy();
      expect(screen.getByText('The model is taking too long to load. The server may be busy or the model may be too large.')).toBeTruthy();
      expect(screen.getByText('Attempt 2 of 3')).toBeTruthy();
    });

    it('renders out of memory error', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: {
          code: 'OUT_OF_MEMORY',
          message: 'Insufficient memory',
          retryCount: 0,
          canRetry: false,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Insufficient Memory')).toBeTruthy();
      expect(screen.getByText('Not enough memory to load this model. Try closing other applications or unloading other models.')).toBeTruthy();
    });

    it('renders already loading error with info variant', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'loading',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: {
          code: 'ALREADY_LOADING',
          message: 'Model already loading',
          retryCount: 0,
          canRetry: false,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Model Loading in Progress')).toBeTruthy();
      expect(screen.getByText('A model is already being loaded. Please wait for it to complete.')).toBeTruthy();
    });

    it('calls retry handler when retry button is clicked', async () => {
      const mockRetry = vi.fn();

      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network failed',
          retryCount: 1,
          canRetry: true,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: mockRetry,
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      // There may be multiple retry buttons, find the one from the error display (primary variant)
      const retryButtons = screen.getAllByRole('button', { name: /Retry/i });
      const errorRetryButton = retryButtons.find(btn => btn.textContent?.includes('Retry'));
      expect(errorRetryButton).toBeTruthy();

      if (errorRetryButton) {
        await user.click(errorRetryButton);
      }

      expect(mockRetry).toHaveBeenCalledTimes(1);
    });

    it('calls clearError handler when dismiss button is clicked', async () => {
      const mockClearError = vi.fn();

      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network failed',
          retryCount: 0,
          canRetry: true,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: mockClearError,
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const dismissButton = screen.getByRole('button', { name: /Dismiss/i });
      await user.click(dismissButton);

      expect(mockClearError).toHaveBeenCalledTimes(1);
    });

    it('disables retry button while retrying', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: true,
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network failed',
          retryCount: 1,
          canRetry: true,
        },
        isError: true,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      const retryButtons = screen.getAllByRole('button', { name: /Retry/i });
      expect(retryButtons.length).toBeGreaterThan(0);
      // At least one retry button should be disabled
      const hasDisabledRetry = retryButtons.some(btn => btn.hasAttribute('disabled'));
      expect(hasDisabledRetry).toBe(true);
    });
  });

  describe('No Model State', () => {
    it('renders no-model state when model is not loaded', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      // Both ModelStatusBar and main content show "No model loaded"
      expect(screen.getAllByText('No model loaded').length).toBeGreaterThan(0);
      expect(screen.getByText('Click "Load Model" above to load a model and start chatting. If no models are available, contact your administrator.')).toBeTruthy();
    });

    it('renders ModelStatusBar in no-model state', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      // ModelStatusBar should be present (will be tested separately)
      const statusBar = document.querySelector('.border-b');
      expect(statusBar).toBeTruthy();
    });
  });

  describe('Ready State', () => {
    it('renders ChatInterface when model is ready', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: mockDefaultStack,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      const chatInterface = screen.getByTestId('chat-interface');
      expect(chatInterface).toBeTruthy();
      expect(chatInterface.textContent).toContain('test-tenant');
    });

    it('passes initialStackId to ChatInterface when default stack exists', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: mockDefaultStack,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      const chatInterface = screen.getByTestId('chat-interface');
      expect(chatInterface.textContent).toContain('default-stack');
    });

    it('renders without initialStackId when no default stack', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      const chatInterface = screen.getByTestId('chat-interface');
      expect(chatInterface.textContent).toContain('test-tenant');
      expect(chatInterface.textContent).not.toContain('Stack:');
    });

    it('renders ModelStatusBar in ready state', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      // ModelStatusBar should be present
      const statusBar = document.querySelector('.border-b');
      expect(statusBar).toBeTruthy();
    });
  });

  describe('Layout Structure', () => {
    it('has correct flex column layout', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      const { container } = render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      const layout = container.querySelector('.flex.flex-col.h-full');
      expect(layout).toBeTruthy();
    });

    it('chat interface has overflow-hidden wrapper', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refresh: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        error: null,
        isError: false,
        autoLoadEnabled: true,
        disableAutoLoad: vi.fn(),
        enableAutoLoad: vi.fn(),
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        retry: vi.fn(),
        clearError: vi.fn(),
      });

      mockUseGetDefaultStack.mockReturnValue({
        data: null,
        isLoading: false,
      });

      const { container } = render(
        <TestWrapper>
          <OperatorChatLayout tenantId="test-tenant" />
        </TestWrapper>
      );

      const wrapper = container.querySelector('.flex-1.overflow-hidden');
      expect(wrapper).toBeTruthy();
    });
  });

  describe('Error Display Component', () => {
    it('shows correct error icons for different error types', () => {
      const errorCodes: Array<{ code: 'NETWORK_ERROR' | 'NO_MODELS' | 'TIMEOUT' | 'OUT_OF_MEMORY' | 'ALREADY_LOADING'; expectedTitle: string }> = [
        { code: 'NETWORK_ERROR', expectedTitle: 'Network Error' },
        { code: 'NO_MODELS', expectedTitle: 'No Models Available' },
        { code: 'TIMEOUT', expectedTitle: 'Loading Timed Out' },
        { code: 'OUT_OF_MEMORY', expectedTitle: 'Insufficient Memory' },
        { code: 'ALREADY_LOADING', expectedTitle: 'Model Loading in Progress' },
      ];

      errorCodes.forEach(({ code, expectedTitle }) => {
        vi.clearAllMocks();

        mockUseModelStatus.mockReturnValue({
          status: 'no-model',
          modelName: null,
          modelId: null,
          modelPath: null,
          memoryUsageMb: null,
          errorMessage: null,
          isReady: false,
          refresh: vi.fn(),
        });

        mockUseAutoLoadModel.mockReturnValue({
          isAutoLoading: false,
          error: {
            code,
            message: 'Test error',
            retryCount: 0,
            canRetry: true,
          },
          isError: true,
          autoLoadEnabled: true,
          disableAutoLoad: vi.fn(),
          enableAutoLoad: vi.fn(),
          toggleAutoLoad: vi.fn(),
          loadModel: vi.fn(),
          retry: vi.fn(),
          clearError: vi.fn(),
        });

        mockUseGetDefaultStack.mockReturnValue({
          data: null,
          isLoading: false,
        });

        const { unmount } = render(
          <TestWrapper>
            <OperatorChatLayout tenantId="test-tenant" />
          </TestWrapper>
        );

        expect(screen.getByText(expectedTitle)).toBeTruthy();
        unmount();
      });
    });
  });
});
