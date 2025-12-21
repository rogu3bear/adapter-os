/**
 * ModelStatusBar Tests
 *
 * Comprehensive tests for the ModelStatusBar component:
 * - Component renders correctly in different states
 * - Model status display (checking, no-model, loading, ready, unloading, error)
 * - Memory usage formatting
 * - Load/Unload button functionality
 * - Auto-load toggle functionality
 * - Retry button on auto-load errors
 * - Tooltips and status badges
 * - Operation state management
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ModelStatusBar } from '@/components/operator';
import { toast } from 'sonner';

// Mock API client
const mockUnloadBaseModel = vi.fn();

vi.mock('@/api/services', () => ({
  apiClient: {
    unloadBaseModel: (...args: unknown[]) => mockUnloadBaseModel(...args),
  },
}));

// Mock hooks
const mockUseModelStatus = vi.fn();
const mockUseAutoLoadModel = vi.fn();

vi.mock('@/hooks/model-loading', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/hooks/model-loading')>();
  return {
    ...original,
    useModelStatus: (tenantId: string) => mockUseModelStatus(tenantId),
    useAutoLoadModel: (tenantId: string, enabled: boolean) => mockUseAutoLoadModel(tenantId, enabled),
  };
});

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

describe('ModelStatusBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    mockUnloadBaseModel.mockResolvedValue({});
  });

  describe('Status Display', () => {
    it('renders checking state with skeleton loader', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'checking',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      // Should show skeleton with loading aria-label
      const skeleton = screen.getByLabelText('Loading');
      expect(skeleton).toBeTruthy();
      expect(skeleton.getAttribute('data-slot')).toBe('skeleton');
    });

    it('renders no-model state', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('No model loaded')).toBeTruthy();
      expect(screen.getByText('No Model')).toBeTruthy();
      expect(screen.getByRole('button', { name: /Load Model/i })).toBeTruthy();
    });

    it('renders loading state', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'loading',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Test Model')).toBeTruthy();
      expect(screen.getByText('Loading')).toBeTruthy();

      // Should show spinner in badge
      const spinners = document.querySelectorAll('.animate-spin');
      expect(spinners.length).toBeGreaterThan(0);
    });

    it('renders ready state with memory usage', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Test Model')).toBeTruthy();
      expect(screen.getByText('Ready')).toBeTruthy();
      expect(screen.getByText('2.0 GB')).toBeTruthy();
      expect(screen.getByRole('button', { name: /Unload/i })).toBeTruthy();
    });

    it('renders unloading state', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'unloading',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Test Model')).toBeTruthy();
      expect(screen.getByText('Unloading')).toBeTruthy();

      // Should show spinner in badge
      const spinners = document.querySelectorAll('.animate-spin');
      expect(spinners.length).toBeGreaterThan(0);
    });

    it('renders error state with error message', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'error',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: null,
        errorMessage: 'Model failed to load',
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Test Model')).toBeTruthy();
      expect(screen.getByText('Error')).toBeTruthy();
      expect(screen.getByText('Model failed to load')).toBeTruthy();
    });
  });

  describe('Memory Formatting', () => {
    it('formats memory in MB when less than 1024 MB', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Small Model',
        modelId: 'small-model',
        modelPath: '/path/to/model',
        memoryUsageMb: 512,
        errorMessage: null,
        isReady: true,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('512 MB')).toBeTruthy();
    });

    it('formats memory in GB when 1024 MB or more', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Large Model',
        modelId: 'large-model',
        modelPath: '/path/to/model',
        memoryUsageMb: 4096,
        errorMessage: null,
        isReady: true,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('4.0 GB')).toBeTruthy();
    });

    it('shows em dash when memory is null', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'loading',
        modelName: 'Test Model',
        modelId: 'test-model',
        modelPath: '/path/to/model',
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      // Memory should not be shown when not ready
      expect(screen.queryByText(/MB|GB/)).toBeNull();
    });

    it('only shows memory when status is ready', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'loading',
        modelName: 'Test Model',
        modelId: 'test-model',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      // Memory should not be shown when loading
      expect(screen.queryByText('2.0 GB')).toBeNull();
    });
  });

  describe('Load Button', () => {
    it('calls loadModel when Load Model button is clicked', async () => {
      const mockLoadModel = vi.fn();

      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: mockLoadModel,
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const loadButton = screen.getByRole('button', { name: /Load Model/i });
      await user.click(loadButton);

      expect(mockLoadModel).toHaveBeenCalledTimes(1);
    });

    it('disables load button when loading', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'loading',
        modelName: 'Test Model',
        modelId: 'test-model',
        modelPath: '/path/to/model',
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const loadButton = screen.getByRole('button', { name: /Loading.../i });
      expect(loadButton.hasAttribute('disabled')).toBe(true);
    });

    it('disables load button when checking status', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'checking',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const loadButton = screen.getByRole('button', { name: /Load Model/i });
      expect(loadButton.hasAttribute('disabled')).toBe(true);
    });

    it('shows loading spinner when auto-loading', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: true,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByRole('button', { name: /Loading.../i })).toBeTruthy();

      const spinners = document.querySelectorAll('.animate-spin');
      expect(spinners.length).toBeGreaterThan(0);
    });
  });

  describe('Unload Button', () => {
    it('shows unload button when model is ready', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByRole('button', { name: /Unload/i })).toBeTruthy();
    });

    it('calls unloadBaseModel when unload button is clicked', async () => {
      const mockRefetch = vi.fn().mockResolvedValue({});

      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refetch: mockRefetch,
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      mockUnloadBaseModel.mockResolvedValue({});

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const unloadButton = screen.getByRole('button', { name: /Unload/i });
      await user.click(unloadButton);

      await waitFor(() => {
        expect(mockUnloadBaseModel).toHaveBeenCalledWith('test-model-id');
        expect(toast.success).toHaveBeenCalledWith('Model unloaded');
        expect(mockRefetch).toHaveBeenCalled();
      });
    });

    it('shows error toast when unload fails', async () => {
      const mockRefresh = vi.fn();

      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refetch: mockRefresh,
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      mockUnloadBaseModel.mockRejectedValue(new Error('Unload failed'));

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const unloadButton = screen.getByRole('button', { name: /Unload/i });
      await user.click(unloadButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('Failed to unload: Unload failed');
      });
    });

    it('disables unload button during unload operation', async () => {
      const mockRefetch = vi.fn().mockResolvedValue({});

      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refetch: mockRefetch,
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      // Mock unload to take some time
      mockUnloadBaseModel.mockImplementation(() => new Promise(resolve => setTimeout(resolve, 100)));

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const unloadButton = screen.getByRole('button', { name: /Unload/i });

      // Start unload
      user.click(unloadButton);

      // Button should be disabled during operation
      await waitFor(() => {
        const button = screen.getByRole('button', { name: /Unload/i });
        expect(button).toBeDisabled();
      });
    });

    it('does not call unload if modelId is missing', async () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: null,
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const unloadButton = screen.getByRole('button', { name: /Unload/i });
      await user.click(unloadButton);

      expect(mockUnloadBaseModel).not.toHaveBeenCalled();
    });
  });

  describe('Auto-Load Toggle', () => {
    it('renders auto-load toggle switch', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Auto-load')).toBeTruthy();
      const toggle = screen.getByRole('switch', { name: /Auto-load model on login/i });
      expect(toggle).toBeTruthy();
    });

    it('calls toggleAutoLoad when switch is clicked', async () => {
      const mockToggleAutoLoad = vi.fn();

      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: mockToggleAutoLoad,
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const toggle = screen.getByRole('switch', { name: /Auto-load model on login/i });
      await user.click(toggle);

      expect(mockToggleAutoLoad).toHaveBeenCalledTimes(1);
    });

    it('disables toggle during operations', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'loading',
        modelName: 'Test Model',
        modelId: 'test-model',
        modelPath: '/path/to/model',
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const toggle = screen.getByRole('switch', { name: /Auto-load model on login/i });
      expect(toggle.hasAttribute('disabled')).toBe(true);
    });

    it('reflects autoLoadEnabled state', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: false,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const toggle = screen.getByRole('switch', { name: /Auto-load model on login/i });
      expect(toggle.getAttribute('data-state')).toBe('unchecked');
    });
  });

  describe('Retry Button', () => {
    it('shows retry button when auto-load error can retry', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network failed',
          retryCount: 1,
          canRetry: true,
        },
        isError: true,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByRole('button', { name: /Retry/i })).toBeTruthy();
    });

    it('does not show retry button when error cannot retry', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: {
          code: 'NO_MODELS',
          message: 'No models available',
          retryCount: 0,
          canRetry: false,
        },
        isError: true,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.queryByRole('button', { name: /Retry/i })).toBeNull();
    });

    it('calls retry when retry button is clicked', async () => {
      const mockRetry = vi.fn();

      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network failed',
          retryCount: 1,
          canRetry: true,
        },
        isError: true,
        retry: mockRetry,
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const retryButton = screen.getByRole('button', { name: /Retry/i });
      await user.click(retryButton);

      expect(mockRetry).toHaveBeenCalledTimes(1);
    });

    it('disables retry button during operation', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: true,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network failed',
          retryCount: 1,
          canRetry: true,
        },
        isError: true,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const retryButton = screen.getByRole('button', { name: /Retry/i });
      expect(retryButton.hasAttribute('disabled')).toBe(true);
    });

    it('shows auto-load error badge with tooltip', async () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
        modelName: null,
        modelId: null,
        modelPath: null,
        memoryUsageMb: null,
        errorMessage: null,
        isReady: false,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: {
          code: 'NETWORK_ERROR',
          message: 'Network connection failed',
          retryCount: 2,
          canRetry: true,
        },
        isError: true,
        retry: vi.fn(),
      });

      render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      expect(screen.getByText('Network error')).toBeTruthy();
    });
  });

  describe('Layout and Styling', () => {
    it('has correct flex layout with border-b', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      const { container } = render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      const statusBar = container.querySelector('.border-b');
      expect(statusBar).toBeTruthy();
      expect(statusBar?.classList.contains('flex')).toBe(true);
    });

    it('displays CPU icon', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Test Model',
        modelId: 'test-model-id',
        modelPath: '/path/to/model',
        memoryUsageMb: 2048,
        errorMessage: null,
        isReady: true,
        refetch: vi.fn(),
      });

      mockUseAutoLoadModel.mockReturnValue({
        isAutoLoading: false,
        autoLoadEnabled: true,
        toggleAutoLoad: vi.fn(),
        loadModel: vi.fn(),
        error: null,
        isError: false,
        retry: vi.fn(),
      });

      const { container } = render(
        <TestWrapper>
          <ModelStatusBar tenantId="test-tenant" />
        </TestWrapper>
      );

      // CPU icon should be present (using SVG class selectors)
      const icons = container.querySelectorAll('svg');
      expect(icons.length).toBeGreaterThan(0);
    });
  });
});
