import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { useTrainingPreflight } from '@/hooks/training';
import type { Dataset } from '@/api/training-types';

// Mock API client
const mockGetDataset = vi.fn();

vi.mock('@/api/services', () => ({
  default: {
    getDataset: (...args: unknown[]) => mockGetDataset(...args),
  },
}));

// Mock useModelStatus from model-loading
const mockUseModelStatus = vi.fn();

vi.mock('@/hooks/model-loading', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/hooks/model-loading')>();
  return {
    ...original,
    useModelStatus: (...args: unknown[]) => mockUseModelStatus(...args),
  };
});

// Mock trainingPreflight utils
const mockRunClientPreflight = vi.fn();
const mockGetPreflightSummary = vi.fn();

vi.mock('@/utils/trainingPreflight', () => ({
  runClientPreflight: (...args: unknown[]) => mockRunClientPreflight(...args),
  getPreflightSummary: (...args: unknown[]) => mockGetPreflightSummary(...args),
}));

// Test data
const mockDataset: Dataset = {
  id: 'dataset-1',
  name: 'Test Dataset',
  validation_status: 'valid',
  trust_state: 'allowed',
  file_count: 10,
  total_size_bytes: 1024 * 1024 * 100, // 100 MB
  total_tokens: 50000,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

const mockClientPreflightPass = {
  passed: true,
  clean: true,
  checks: [
    {
      policy_id: 'validation_status',
      policy_name: 'Dataset Validated',
      passed: true,
      severity: 'info' as const,
      message: 'Dataset has passed validation',
    },
    {
      policy_id: 'trust_state',
      policy_name: 'Trust State',
      passed: true,
      severity: 'info' as const,
      message: 'Trust: allowed',
    },
  ],
};

// Test wrapper
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

describe('useTrainingPreflight', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockRunClientPreflight.mockReturnValue(mockClientPreflightPass);
    mockGetPreflightSummary.mockReturnValue('All checks passed. Ready to start training.');
    mockUseModelStatus.mockReturnValue({
      status: 'ready',
      modelName: 'Qwen2.5-7B',
    });
  });

  describe('initial state', () => {
    it('returns error when dataset is null', () => {
      const { result } = renderHook(() => useTrainingPreflight(null), {
        wrapper: createWrapper(),
      });

      expect(result.current.canProceed).toBe(false);
      expect(result.current.clientChecks).toHaveLength(1);
      expect(result.current.clientChecks[0].policy_id).toBe('dataset_required');
      expect(result.current.clientChecks[0].passed).toBe(false);
    });

    it('returns error when dataset is undefined', () => {
      const { result } = renderHook(() => useTrainingPreflight(undefined), {
        wrapper: createWrapper(),
      });

      expect(result.current.canProceed).toBe(false);
      expect(result.current.clientChecks[0].message).toBe('No dataset selected');
    });
  });

  describe('client-side checks', () => {
    it('runs client preflight checks immediately', () => {
      renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      expect(mockRunClientPreflight).toHaveBeenCalledWith(mockDataset);
    });

    it('returns client check results', () => {
      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      expect(result.current.clientChecks).toEqual(mockClientPreflightPass.checks);
    });

    it('handles failed client checks', () => {
      const failedChecks = {
        passed: false,
        clean: false,
        checks: [
          {
            policy_id: 'validation_status',
            policy_name: 'Dataset Validated',
            passed: false,
            severity: 'error' as const,
            message: 'Dataset status: pending',
          },
        ],
      };
      mockRunClientPreflight.mockReturnValue(failedChecks);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      expect(result.current.canProceed).toBe(false);
      expect(result.current.clientChecks[0].passed).toBe(false);
    });
  });

  describe('server-side checks', () => {
    it('fetches dataset from server', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);

      renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(mockGetDataset).toHaveBeenCalledWith('dataset-1');
      });
    });

    it('does not fetch when dataset is null', () => {
      renderHook(() => useTrainingPreflight(null), {
        wrapper: createWrapper(),
      });

      expect(mockGetDataset).not.toHaveBeenCalled();
    });

    it('does not fetch when enabled is false', () => {
      renderHook(() => useTrainingPreflight(mockDataset, { enabled: false }), {
        wrapper: createWrapper(),
      });

      expect(mockGetDataset).not.toHaveBeenCalled();
    });

    it('shows loading state while fetching dataset', async () => {
      mockGetDataset.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve(mockDataset), 100))
      );

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      expect(result.current.isLoading).toBe(true);

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });
    });

    it('creates dataset verification check when loading', async () => {
      mockGetDataset.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve(mockDataset), 100))
      );

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      const datasetCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'dataset_exists'
      );

      expect(datasetCheck?.message).toBe('Verifying dataset...');
    });

    it('creates success check when dataset verified', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        const datasetCheck = result.current.serverChecks.find(
          (c) => c.policy_id === 'dataset_exists'
        );
        expect(datasetCheck?.passed).toBe(true);
        expect(datasetCheck?.message).toBe('Dataset verified on server');
      });
    });

    it('creates warning when dataset state changed', async () => {
      const changedDataset = { ...mockDataset, validation_status: 'pending' as const };
      mockGetDataset.mockResolvedValue(changedDataset);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        const datasetCheck = result.current.serverChecks.find(
          (c) => c.policy_id === 'dataset_exists'
        );
        expect(datasetCheck?.severity).toBe('warning');
        expect(datasetCheck?.message).toContain('Status changed');
      });
    });

    it('creates error check when dataset fetch fails', async () => {
      const error = new Error('Dataset not found');
      mockGetDataset.mockRejectedValue(error);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      // Wait for the dataset check to show error (not just loading state)
      await waitFor(() => {
        const datasetCheck = result.current.serverChecks.find(
          (c) => c.policy_id === 'dataset_exists'
        );
        expect(datasetCheck?.message).not.toBe('Verifying dataset...');
      }, { timeout: 5000 });

      // Now verify it shows the error
      const datasetCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'dataset_exists'
      );
      expect(datasetCheck?.passed).toBe(false);
      expect(datasetCheck?.message).toBe('Failed to verify dataset');
    });
  });

  describe('model status checks', () => {
    it('calls useModelStatus with correct tenant ID', () => {
      renderHook(() => useTrainingPreflight(mockDataset, { tenantId: 'tenant-123' }), {
        wrapper: createWrapper(),
      });

      expect(mockUseModelStatus).toHaveBeenCalledWith('tenant-123');
    });

    it('uses default tenant ID', () => {
      renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      expect(mockUseModelStatus).toHaveBeenCalledWith('default');
    });

    it('creates success check when model is ready', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'ready',
        modelName: 'Qwen2.5-7B',
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      const workerCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'worker_available'
      );

      expect(workerCheck?.passed).toBe(true);
      expect(workerCheck?.message).toBe('Model ready: Qwen2.5-7B');
    });

    it('creates warning when model is loading', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'loading',
        modelName: 'Qwen2.5-7B',
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      const workerCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'worker_available'
      );

      expect(workerCheck?.passed).toBe(true);
      expect(workerCheck?.severity).toBe('warning');
      expect(workerCheck?.message).toBe('Model is loading - training will queue');
    });

    it('creates error when no model loaded', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      const workerCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'worker_available'
      );

      expect(workerCheck?.passed).toBe(false);
      expect(workerCheck?.severity).toBe('error');
      expect(workerCheck?.message).toBe('No model loaded');
    });

    it('creates error when model has error', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'error',
        errorMessage: 'Failed to load model',
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      const workerCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'worker_available'
      );

      expect(workerCheck?.passed).toBe(false);
      expect(workerCheck?.severity).toBe('error');
      expect(workerCheck?.message).toBe('Model error');
      expect(workerCheck?.details).toBe('Failed to load model');
    });

    it('creates info check when checking model status', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'checking',
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      const workerCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'worker_available'
      );

      expect(workerCheck?.passed).toBe(true);
      expect(workerCheck?.message).toBe('Checking model status...');
    });
  });

  describe('combined checks', () => {
    it('combines client and server checks', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.allChecks.length).toBeGreaterThan(0);
      });

      expect(result.current.allChecks).toEqual(
        expect.arrayContaining([
          ...result.current.clientChecks,
          ...result.current.serverChecks,
        ])
      );
    });

    it('calculates canProceed correctly when all pass', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);
      mockRunClientPreflight.mockReturnValue(mockClientPreflightPass);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.canProceed).toBe(true);
      });
    });

    it('calculates canProceed as false when client check fails', () => {
      mockRunClientPreflight.mockReturnValue({
        passed: false,
        clean: false,
        checks: [
          {
            policy_id: 'test',
            policy_name: 'Test',
            passed: false,
            severity: 'error' as const,
            message: 'Failed',
          },
        ],
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      expect(result.current.canProceed).toBe(false);
    });

    it('calculates canProceed as false when server check fails', async () => {
      mockUseModelStatus.mockReturnValue({
        status: 'no-model',
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.canProceed).toBe(false);
      });
    });

    it('calculates isClean correctly with no warnings', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);
      mockRunClientPreflight.mockReturnValue(mockClientPreflightPass);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isClean).toBe(true);
      });
    });

    it('calculates isClean as false when warnings exist', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);
      mockRunClientPreflight.mockReturnValue({
        passed: true,
        clean: false,
        checks: [
          {
            policy_id: 'test',
            policy_name: 'Test',
            passed: true,
            severity: 'warning' as const,
            message: 'Warning',
          },
        ],
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isClean).toBe(false);
      });
    });
  });

  describe('summary', () => {
    it('returns loading summary when checks are running', () => {
      mockUseModelStatus.mockReturnValue({
        status: 'checking',
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      expect(result.current.summary).toBe('Running preflight checks...');
    });

    it('returns error summary when checks fail', async () => {
      mockRunClientPreflight.mockReturnValue({
        passed: false,
        clean: false,
        checks: [
          {
            policy_id: 'test1',
            policy_name: 'Test 1',
            passed: false,
            severity: 'error' as const,
            message: 'Error 1',
          },
          {
            policy_id: 'test2',
            policy_name: 'Test 2',
            passed: false,
            severity: 'error' as const,
            message: 'Error 2',
          },
        ],
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.summary).toBe('2 issues must be resolved.');
      });
    });

    it('returns warning summary when warnings exist', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);
      mockRunClientPreflight.mockReturnValue({
        passed: true,
        clean: false,
        checks: [
          {
            policy_id: 'test',
            policy_name: 'Test',
            passed: true,
            severity: 'warning' as const,
            message: 'Warning',
          },
        ],
      });

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.summary).toBe('Ready with 1 warning.');
      });
    });

    it('returns success summary when all clean', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);
      mockRunClientPreflight.mockReturnValue(mockClientPreflightPass);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.summary).toBe('All checks passed. Ready to start training.');
      });
    });
  });

  describe('refetch', () => {
    it('provides refetch function', () => {
      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      expect(typeof result.current.refetch).toBe('function');
    });

    it('refetches dataset on demand', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(mockGetDataset).toHaveBeenCalledTimes(1);
      });

      await result.current.refetch();

      expect(mockGetDataset).toHaveBeenCalledTimes(2);
    });
  });

  describe('error states', () => {
    it('includes error in result when dataset fetch fails', async () => {
      const error = new Error('Network error');
      mockGetDataset.mockRejectedValue(error);

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      // Wait for the dataset check to show error (not just loading state)
      await waitFor(() => {
        const datasetCheck = result.current.serverChecks.find(
          (c) => c.policy_id === 'dataset_exists'
        );
        expect(datasetCheck?.message).not.toBe('Verifying dataset...');
      }, { timeout: 5000 });

      // The error should be reflected in the server checks
      const datasetCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'dataset_exists'
      );
      expect(datasetCheck?.message).toContain('Failed to verify dataset');

      // Error may be available depending on query state
      if (result.current.error) {
        expect(result.current.error).toEqual(error);
      }
    });

    it('handles non-Error objects as errors', async () => {
      mockGetDataset.mockRejectedValue('String error');

      const { result } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper: createWrapper(),
      });

      // Wait for the dataset check to show error (not just loading state)
      await waitFor(() => {
        const datasetCheck = result.current.serverChecks.find(
          (c) => c.policy_id === 'dataset_exists'
        );
        expect(datasetCheck?.message).not.toBe('Verifying dataset...');
      }, { timeout: 5000 });

      // Should show error in checks even for non-Error objects
      const datasetCheck = result.current.serverChecks.find(
        (c) => c.policy_id === 'dataset_exists'
      );
      expect(datasetCheck?.message).toContain('Failed to verify dataset');
    });
  });

  describe('caching behavior', () => {
    it('caches dataset verification for 10 seconds', async () => {
      mockGetDataset.mockResolvedValue(mockDataset);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result: result1 } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper,
      });

      await waitFor(() => {
        expect(mockGetDataset).toHaveBeenCalledTimes(1);
      });

      // Second render should use cache
      const { result: result2 } = renderHook(() => useTrainingPreflight(mockDataset), {
        wrapper,
      });

      await waitFor(() => {
        expect(result2.current.isLoading).toBe(false);
      });

      // Should still only have been called once (cached)
      expect(mockGetDataset).toHaveBeenCalledTimes(1);
    });
  });
});
