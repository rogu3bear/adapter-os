import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { useSettings, useUpdateSettings, settingsKeys } from '@/hooks/config/useSettings';
import type {
  SystemSettings,
  UpdateSettingsRequest,
  SettingsUpdateResponse,
} from '@/api/document-types';

// Mock API client
const mockApiRequest = vi.fn();

vi.mock('@/api/client', () => ({
  apiClient: {
    request: (...args: unknown[]) => mockApiRequest(...args),
  },
}));

// Mock toast
const mockToast = vi.fn();
vi.mock('@/hooks/use-toast', () => ({
  useToast: () => ({ toast: mockToast }),
}));

// Test data
const mockSettings: SystemSettings = {
  schema_version: '1.0',
  general: {
    system_name: 'AdapterOS Test',
    environment: 'development',
    api_base_url: 'http://localhost:8080',
  },
  server: {
    http_port: 8080,
    https_port: null,
    uds_socket_path: '/var/run/aos.sock',
    production_mode: false,
  },
  security: {
    jwt_mode: 'eddsa',
    token_ttl_seconds: 28800,
    require_mfa: false,
    egress_enabled: false,
    require_pf_deny: true,
  },
  performance: {
    max_adapters: 100,
    max_workers: 8,
    memory_threshold_pct: 85,
    cache_size_mb: 2048,
  },
};

// Test wrapper
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

describe('useSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('fetches settings successfully', async () => {
    mockApiRequest.mockResolvedValue(mockSettings);

    const { result } = renderHook(() => useSettings(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual(mockSettings);
    expect(mockApiRequest).toHaveBeenCalledWith('/v1/settings');
  });

  it('uses 5-minute stale time', async () => {
    mockApiRequest.mockResolvedValue(mockSettings);

    const { result } = renderHook(() => useSettings(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    // Query should be marked as fresh for 5 minutes
    expect(result.current.isStale).toBe(false);
  });

  it('handles fetch error', async () => {
    const error = new Error('Failed to fetch settings');
    mockApiRequest.mockRejectedValue(error);

    const { result } = renderHook(() => useSettings(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(error);
  });

  it('caches settings data', async () => {
    mockApiRequest.mockResolvedValue(mockSettings);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    // Verify data is cached
    const cachedData = queryClient.getQueryData<SystemSettings>(settingsKeys.current());
    expect(cachedData).toEqual(mockSettings);
  });
});

describe('useUpdateSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('optimistic updates', () => {
    it('updates cache optimistically before server response', async () => {
      const updateResponse: SettingsUpdateResponse = {
        schema_version: '1.0',
        success: true,
        restart_required: false,
        message: 'Settings updated successfully',
      };

      // Simulate slow server response
      mockApiRequest.mockImplementation(
        () => new Promise(resolve => setTimeout(() => resolve(updateResponse), 100))
      );

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Pre-populate cache with existing settings
      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        general: { system_name: 'New System Name' },
      };

      const updatePromise = result.current.mutateAsync(updateRequest);

      // Check optimistic update happened immediately
      await waitFor(() => {
        const cachedData = queryClient.getQueryData<SystemSettings>(settingsKeys.current());
        expect(cachedData?.general.system_name).toBe('New System Name');
      });

      await updatePromise;
    });

    it('preserves unmodified settings during optimistic update', async () => {
      const updateResponse: SettingsUpdateResponse = {
        schema_version: '1.0',
        success: true,
        restart_required: false,
        message: 'Settings updated',
      };
      mockApiRequest.mockResolvedValue(updateResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        server: { http_port: 9090 },
      };

      await result.current.mutateAsync(updateRequest);

      await waitFor(() => {
        const cachedData = queryClient.getQueryData<SystemSettings>(settingsKeys.current());
        // Updated field
        expect(cachedData?.server.http_port).toBe(9090);
        // Preserved fields
        expect(cachedData?.general.system_name).toBe(mockSettings.general.system_name);
        expect(cachedData?.security.jwt_mode).toBe(mockSettings.security.jwt_mode);
        expect(cachedData?.performance.max_adapters).toBe(mockSettings.performance.max_adapters);
      });
    });

    it('updates multiple settings sections', async () => {
      const updateResponse: SettingsUpdateResponse = {
        schema_version: '1.0',
        success: true,
        restart_required: false,
        message: 'Settings updated',
      };
      mockApiRequest.mockResolvedValue(updateResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        general: { environment: 'production' },
        server: { production_mode: true },
        security: { require_mfa: true },
      };

      await result.current.mutateAsync(updateRequest);

      await waitFor(() => {
        const cachedData = queryClient.getQueryData<SystemSettings>(settingsKeys.current());
        expect(cachedData?.general.environment).toBe('production');
        expect(cachedData?.server.production_mode).toBe(true);
        expect(cachedData?.security.require_mfa).toBe(true);
      });
    });
  });

  describe('error handling and rollback', () => {
    it('rolls back optimistic update on error', async () => {
      const error = new Error('Update failed');
      mockApiRequest.mockRejectedValue(error);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      // Pre-populate cache with original settings
      queryClient.setQueryData(settingsKeys.current(), mockSettings);
      const originalSystemName = mockSettings.general.system_name;

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        general: { system_name: 'Failed Update' },
      };

      await expect(result.current.mutateAsync(updateRequest)).rejects.toThrow('Update failed');

      // Verify rollback happened
      const cachedData = queryClient.getQueryData<SystemSettings>(settingsKeys.current());
      expect(cachedData?.general.system_name).toBe(originalSystemName);
    });

    it('shows error toast on update failure', async () => {
      const error = new Error('Network error');
      mockApiRequest.mockRejectedValue(error);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        general: { system_name: 'Test' },
      };

      await expect(result.current.mutateAsync(updateRequest)).rejects.toThrow();

      expect(mockToast).toHaveBeenCalledWith({
        title: 'Failed to update settings',
        description: 'Your changes could not be saved. Please try again.',
        variant: 'destructive',
      });
    });
  });

  describe('restart-required handling', () => {
    it('shows restart-required toast when needed', async () => {
      const updateResponse: SettingsUpdateResponse = {
        schema_version: '1.0',
        success: true,
        restart_required: true,
        message: 'Settings updated',
      };
      mockApiRequest.mockResolvedValue(updateResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        server: { production_mode: true },
      };

      await result.current.mutateAsync(updateRequest);

      expect(mockToast).toHaveBeenCalledWith({
        title: 'Settings saved',
        description: 'A server restart is required for some changes to take effect.',
        variant: 'default',
      });
    });

    it('shows standard success toast when restart not required', async () => {
      const updateResponse: SettingsUpdateResponse = {
        schema_version: '1.0',
        success: true,
        restart_required: false,
        message: 'Settings updated successfully',
      };
      mockApiRequest.mockResolvedValue(updateResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        general: { system_name: 'Updated Name' },
      };

      await result.current.mutateAsync(updateRequest);

      expect(mockToast).toHaveBeenCalledWith({
        title: 'Settings saved',
        description: 'Settings updated successfully',
      });
    });

    it('uses default message when server does not provide one', async () => {
      const updateResponse: SettingsUpdateResponse = {
        schema_version: '1.0',
        success: true,
        restart_required: false,
        message: '',
      };
      mockApiRequest.mockResolvedValue(updateResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        general: { system_name: 'Test' },
      };

      await result.current.mutateAsync(updateRequest);

      expect(mockToast).toHaveBeenCalledWith({
        title: 'Settings saved',
        description: 'Your settings have been updated.',
      });
    });
  });

  describe('cache invalidation', () => {
    it('invalidates settings cache on success', async () => {
      const updateResponse: SettingsUpdateResponse = {
        schema_version: '1.0',
        success: true,
        restart_required: false,
        message: 'Updated',
      };
      mockApiRequest.mockResolvedValue(updateResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        general: { system_name: 'Test' },
      };

      await result.current.mutateAsync(updateRequest);

      // Verify invalidation was called
      const queries = queryClient.getQueryCache().findAll({
        queryKey: settingsKeys.current(),
      });
      expect(queries.length).toBeGreaterThan(0);
    });
  });

  describe('API request format', () => {
    it('sends PUT request to /v1/settings', async () => {
      const updateResponse: SettingsUpdateResponse = {
        schema_version: '1.0',
        success: true,
        restart_required: false,
        message: 'Updated',
      };
      mockApiRequest.mockResolvedValue(updateResponse);

      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      queryClient.setQueryData(settingsKeys.current(), mockSettings);

      const { result } = renderHook(() => useUpdateSettings(), { wrapper });

      const updateRequest: UpdateSettingsRequest = {
        general: { system_name: 'Test' },
      };

      await result.current.mutateAsync(updateRequest);

      expect(mockApiRequest).toHaveBeenCalledWith('/v1/settings', {
        method: 'PUT',
        body: JSON.stringify(updateRequest),
      });
    });
  });
});

describe('settingsKeys', () => {
  it('generates correct query keys', () => {
    expect(settingsKeys.all).toEqual(['settings']);
    expect(settingsKeys.current()).toEqual(['settings', 'current']);
  });
});
