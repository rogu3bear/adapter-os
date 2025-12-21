/**
 * Tests for useTenantPolicies hook
 *
 * Verifies that each endpoint is called correctly with the mocked apiClient.
 * Citation: AGENTS.md - Policy Studio feature UI implementation
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import {
  useTenantCustomizations,
  useTenantCustomization,
  usePendingReviews,
  useCreateCustomization,
  useUpdateCustomization,
  useDeleteCustomization,
  useSubmitForReview,
  useApproveCustomization,
  useRejectCustomization,
  useActivateCustomization,
  tenantPolicyKeys,
  type TenantPolicyCustomization,
  type CustomizationResponse,
} from '@/hooks/security/useTenantPolicies';

// Mock API client
const mockRequest = vi.fn();

vi.mock('@/api/services', () => ({
  apiClient: {
    request: (...args: unknown[]) => mockRequest(...args),
  },
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Test data
const MOCK_TENANT_ID = 'tenant-1';
const MOCK_CUSTOMIZATION_ID = 'custom-1';

const mockCustomization: TenantPolicyCustomization = {
  id: MOCK_CUSTOMIZATION_ID,
  tenant_id: MOCK_TENANT_ID,
  base_policy_type: 'egress',
  customizations_json: '{"max_domains": 10}',
  status: 'draft',
  created_at: '2025-01-01T00:00:00Z',
  created_by: 'user-1',
  updated_at: '2025-01-01T00:00:00Z',
};

const mockCustomizationResponse: CustomizationResponse = {
  customization: mockCustomization,
  validation: {
    valid: true,
    errors: [],
    warnings: [],
  },
};

const mockPendingReviews: TenantPolicyCustomization[] = [
  {
    ...mockCustomization,
    id: 'custom-2',
    status: 'pending_review',
    submitted_at: '2025-01-02T00:00:00Z',
  },
  {
    ...mockCustomization,
    id: 'custom-3',
    status: 'pending_review',
    submitted_at: '2025-01-02T00:00:00Z',
    base_policy_type: 'determinism',
  },
];

// Test wrapper factory
function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
    logger: {
      log: () => {},
      warn: () => {},
      error: () => {},
    },
  });

  return function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
    logger: {
      log: () => {},
      warn: () => {},
      error: () => {},
    },
  });
}

describe('useTenantPolicies - Query Keys', () => {
  it('generates correct query keys', () => {
    expect(tenantPolicyKeys.all).toEqual(['tenant-policies']);
    expect(tenantPolicyKeys.lists()).toEqual(['tenant-policies', 'list']);
    expect(tenantPolicyKeys.list('tenant-1')).toEqual(['tenant-policies', 'list', 'tenant-1']);
    expect(tenantPolicyKeys.listWithFilters('tenant-1', { status: 'draft' })).toEqual([
      'tenant-policies',
      'list',
      'tenant-1',
      { status: 'draft' },
    ]);
    expect(tenantPolicyKeys.details()).toEqual(['tenant-policies', 'detail']);
    expect(tenantPolicyKeys.detail('tenant-1', 'custom-1')).toEqual([
      'tenant-policies',
      'detail',
      'tenant-1',
      'custom-1',
    ]);
    expect(tenantPolicyKeys.pendingReviews()).toEqual(['tenant-policies', 'pending-reviews']);
  });
});

describe('useTenantPolicies - Queries', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useTenantCustomizations', () => {
    it('calls correct endpoint with tenant ID', async () => {
      mockRequest.mockResolvedValue([mockCustomization]);

      const { result } = renderHook(() => useTenantCustomizations(MOCK_TENANT_ID), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customizations?`
      );
      expect(result.current.data).toEqual([mockCustomization]);
    });

    it('appends status filter to query params', async () => {
      mockRequest.mockResolvedValue([mockCustomization]);

      const { result } = renderHook(
        () => useTenantCustomizations(MOCK_TENANT_ID, { status: 'draft' }),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customizations?status=draft`
      );
    });

    it('appends policy_type filter to query params', async () => {
      mockRequest.mockResolvedValue([mockCustomization]);

      const { result } = renderHook(
        () => useTenantCustomizations(MOCK_TENANT_ID, { policy_type: 'egress' }),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customizations?policy_type=egress`
      );
    });

    it('does not fetch when tenantId is empty', () => {
      const { result } = renderHook(() => useTenantCustomizations(''), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockRequest).not.toHaveBeenCalled();
    });

    it('handles API error', async () => {
      const error = new Error('Failed to fetch customizations');
      mockRequest.mockRejectedValue(error);

      const { result } = renderHook(() => useTenantCustomizations(MOCK_TENANT_ID), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isError).toBe(true);
      });

      expect(result.current.error).toEqual(error);
    });
  });

  describe('useTenantCustomization', () => {
    it('calls correct endpoint with tenant and customization IDs', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const { result } = renderHook(
        () => useTenantCustomization(MOCK_TENANT_ID, MOCK_CUSTOMIZATION_ID),
        { wrapper: createWrapper() }
      );

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customizations/${MOCK_CUSTOMIZATION_ID}`
      );
      expect(result.current.data).toEqual(mockCustomizationResponse);
    });

    it('does not fetch when tenantId is empty', () => {
      const { result } = renderHook(() => useTenantCustomization('', MOCK_CUSTOMIZATION_ID), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockRequest).not.toHaveBeenCalled();
    });

    it('does not fetch when customizationId is empty', () => {
      const { result } = renderHook(() => useTenantCustomization(MOCK_TENANT_ID, ''), {
        wrapper: createWrapper(),
      });

      expect(result.current.isPending).toBe(true);
      expect(mockRequest).not.toHaveBeenCalled();
    });
  });

  describe('usePendingReviews', () => {
    it('calls correct endpoint for pending reviews', async () => {
      mockRequest.mockResolvedValue(mockPendingReviews);

      const { result } = renderHook(() => usePendingReviews(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(mockRequest).toHaveBeenCalledWith('/v1/policies/pending-reviews');
      expect(result.current.data).toEqual(mockPendingReviews);
      expect(result.current.data).toHaveLength(2);
    });

    it('returns empty array when no pending reviews', async () => {
      mockRequest.mockResolvedValue([]);

      const { result } = renderHook(() => usePendingReviews(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => {
        expect(result.current.isSuccess).toBe(true);
      });

      expect(result.current.data).toEqual([]);
    });
  });
});

describe('useTenantPolicies - Mutations', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useCreateCustomization', () => {
    it('calls correct endpoint with POST method', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const queryClient = createQueryClient();
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useCreateCustomization(MOCK_TENANT_ID), { wrapper });

      await act(async () => {
        await result.current.mutateAsync({
          base_policy_type: 'egress',
          customizations_json: '{"max_domains": 10}',
        });
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customize`,
        {
          method: 'POST',
          body: JSON.stringify({
            base_policy_type: 'egress',
            customizations_json: '{"max_domains": 10}',
          }),
        }
      );

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.list(MOCK_TENANT_ID),
      });
    });

    it('includes metadata_json when provided', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const { result } = renderHook(() => useCreateCustomization(MOCK_TENANT_ID), {
        wrapper: createWrapper(),
      });

      await act(async () => {
        await result.current.mutateAsync({
          base_policy_type: 'egress',
          customizations_json: '{"max_domains": 10}',
          metadata_json: '{"reason": "testing"}',
        });
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customize`,
        {
          method: 'POST',
          body: JSON.stringify({
            base_policy_type: 'egress',
            customizations_json: '{"max_domains": 10}',
            metadata_json: '{"reason": "testing"}',
          }),
        }
      );
    });
  });

  describe('useUpdateCustomization', () => {
    it('calls correct endpoint with PUT method', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const queryClient = createQueryClient();
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(
        () => useUpdateCustomization(MOCK_TENANT_ID, MOCK_CUSTOMIZATION_ID),
        { wrapper }
      );

      await act(async () => {
        await result.current.mutateAsync({
          customizations_json: '{"max_domains": 20}',
        });
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customizations/${MOCK_CUSTOMIZATION_ID}`,
        {
          method: 'PUT',
          body: JSON.stringify({
            customizations_json: '{"max_domains": 20}',
          }),
        }
      );

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.list(MOCK_TENANT_ID),
      });
      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.detail(MOCK_TENANT_ID, MOCK_CUSTOMIZATION_ID),
      });
    });
  });

  describe('useDeleteCustomization', () => {
    it('calls correct endpoint with DELETE method', async () => {
      mockRequest.mockResolvedValue(undefined);

      const queryClient = createQueryClient();
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useDeleteCustomization(MOCK_TENANT_ID), { wrapper });

      await act(async () => {
        await result.current.mutateAsync(MOCK_CUSTOMIZATION_ID);
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customizations/${MOCK_CUSTOMIZATION_ID}`,
        {
          method: 'DELETE',
        }
      );

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.list(MOCK_TENANT_ID),
      });
    });
  });

  describe('useSubmitForReview', () => {
    it('calls correct endpoint with POST method', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const queryClient = createQueryClient();
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useSubmitForReview(MOCK_TENANT_ID), { wrapper });

      await act(async () => {
        await result.current.mutateAsync(MOCK_CUSTOMIZATION_ID);
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/tenants/${MOCK_TENANT_ID}/policies/customizations/${MOCK_CUSTOMIZATION_ID}/submit`,
        {
          method: 'POST',
        }
      );

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.list(MOCK_TENANT_ID),
      });
      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.pendingReviews(),
      });
    });
  });

  describe('useApproveCustomization', () => {
    it('calls correct endpoint with POST method', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const queryClient = createQueryClient();
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useApproveCustomization(), { wrapper });

      await act(async () => {
        await result.current.mutateAsync({
          customizationId: MOCK_CUSTOMIZATION_ID,
          notes: 'Approved for production use',
        });
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/policies/customizations/${MOCK_CUSTOMIZATION_ID}/approve`,
        {
          method: 'POST',
          body: JSON.stringify({ notes: 'Approved for production use' }),
        }
      );

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.all,
      });
    });

    it('calls endpoint without notes when not provided', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const { result } = renderHook(() => useApproveCustomization(), {
        wrapper: createWrapper(),
      });

      await act(async () => {
        await result.current.mutateAsync({
          customizationId: MOCK_CUSTOMIZATION_ID,
        });
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/policies/customizations/${MOCK_CUSTOMIZATION_ID}/approve`,
        {
          method: 'POST',
          body: JSON.stringify({ notes: undefined }),
        }
      );
    });
  });

  describe('useRejectCustomization', () => {
    it('calls correct endpoint with POST method', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const queryClient = createQueryClient();
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useRejectCustomization(), { wrapper });

      await act(async () => {
        await result.current.mutateAsync({
          customizationId: MOCK_CUSTOMIZATION_ID,
          notes: 'Does not meet compliance requirements',
        });
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/policies/customizations/${MOCK_CUSTOMIZATION_ID}/reject`,
        {
          method: 'POST',
          body: JSON.stringify({ notes: 'Does not meet compliance requirements' }),
        }
      );

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.all,
      });
    });
  });

  describe('useActivateCustomization', () => {
    it('calls correct endpoint with POST method', async () => {
      mockRequest.mockResolvedValue(mockCustomizationResponse);

      const queryClient = createQueryClient();
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
      const wrapper = ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
      );

      const { result } = renderHook(() => useActivateCustomization(), { wrapper });

      await act(async () => {
        await result.current.mutateAsync(MOCK_CUSTOMIZATION_ID);
      });

      expect(mockRequest).toHaveBeenCalledWith(
        `/v1/policies/customizations/${MOCK_CUSTOMIZATION_ID}/activate`,
        {
          method: 'POST',
        }
      );

      expect(invalidateSpy).toHaveBeenCalledWith({
        queryKey: tenantPolicyKeys.all,
      });
    });
  });
});

describe('useTenantPolicies - Error Handling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('handles mutation error for create', async () => {
    const error = new Error('Failed to create customization');
    mockRequest.mockRejectedValue(error);

    const { result } = renderHook(() => useCreateCustomization(MOCK_TENANT_ID), {
      wrapper: createWrapper(),
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync({
          base_policy_type: 'egress',
          customizations_json: '{}',
        });
      })
    ).rejects.toThrow('Failed to create customization');
  });

  it('handles mutation error for approve', async () => {
    const error = new Error('Failed to approve customization');
    mockRequest.mockRejectedValue(error);

    const { result } = renderHook(() => useApproveCustomization(), {
      wrapper: createWrapper(),
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync({ customizationId: MOCK_CUSTOMIZATION_ID });
      })
    ).rejects.toThrow('Failed to approve customization');
  });

  it('handles mutation error for reject', async () => {
    const error = new Error('Failed to reject customization');
    mockRequest.mockRejectedValue(error);

    const { result } = renderHook(() => useRejectCustomization(), {
      wrapper: createWrapper(),
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync({
          customizationId: MOCK_CUSTOMIZATION_ID,
          notes: 'Rejection reason',
        });
      })
    ).rejects.toThrow('Failed to reject customization');
  });
});
