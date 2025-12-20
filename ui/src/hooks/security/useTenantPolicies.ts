// Tenant Policy Customization React Query hooks
// Citation: AGENTS.md - Policy Studio feature UI implementation

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import { toast } from 'sonner';

// Types
export interface TenantPolicyCustomization {
  id: string;
  tenant_id: string;
  base_policy_type: string;
  customizations_json: string;
  status: 'draft' | 'pending_review' | 'approved' | 'rejected' | 'active';
  submitted_at?: string;
  reviewed_at?: string;
  reviewed_by?: string;
  review_notes?: string;
  activated_at?: string;
  created_at: string;
  created_by: string;
  updated_at: string;
  metadata_json?: string;
}

export interface CustomizationResponse {
  customization: TenantPolicyCustomization;
  validation?: {
    valid: boolean;
    errors: string[];
    warnings: string[];
  };
}

export interface CreateCustomizationRequest {
  base_policy_type: string;
  customizations_json: string;
  metadata_json?: string;
}

export interface UpdateCustomizationRequest {
  customizations_json: string;
}

export interface ReviewRequest {
  notes?: string;
}

// Query keys
export const tenantPolicyKeys = {
  all: ['tenant-policies'] as const,
  lists: () => [...tenantPolicyKeys.all, 'list'] as const,
  list: (tenantId: string) => [...tenantPolicyKeys.lists(), tenantId] as const,
  listWithFilters: (tenantId: string, filters: Record<string, unknown>) =>
    [...tenantPolicyKeys.list(tenantId), filters] as const,
  details: () => [...tenantPolicyKeys.all, 'detail'] as const,
  detail: (tenantId: string, id: string) =>
    [...tenantPolicyKeys.details(), tenantId, id] as const,
  pendingReviews: () => [...tenantPolicyKeys.all, 'pending-reviews'] as const,
};

// List tenant customizations
export function useTenantCustomizations(
  tenantId: string,
  params?: { status?: string; policy_type?: string }
) {
  return useQuery({
    queryKey: tenantPolicyKeys.listWithFilters(tenantId, params || {}),
    queryFn: async () => {
      const searchParams = new URLSearchParams();
      if (params?.status) searchParams.append('status', params.status);
      if (params?.policy_type) searchParams.append('policy_type', params.policy_type);

      const response = await apiClient.request<TenantPolicyCustomization[]>(
        `/v1/tenants/${tenantId}/policies/customizations?${searchParams.toString()}`
      );
      return response;
    },
    enabled: !!tenantId,
  });
}

// Get single customization
export function useTenantCustomization(tenantId: string, customizationId: string) {
  return useQuery({
    queryKey: tenantPolicyKeys.detail(tenantId, customizationId),
    queryFn: async () => {
      const response = await apiClient.request<CustomizationResponse>(
        `/v1/tenants/${tenantId}/policies/customizations/${customizationId}`
      );
      return response;
    },
    enabled: !!tenantId && !!customizationId,
  });
}

// List pending reviews (Admin/Compliance only)
export function usePendingReviews() {
  return useQuery({
    queryKey: tenantPolicyKeys.pendingReviews(),
    queryFn: async () => {
      const response = await apiClient.request<TenantPolicyCustomization[]>(
        `/v1/policies/pending-reviews`
      );
      return response;
    },
  });
}

// Create customization
export function useCreateCustomization(tenantId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (data: CreateCustomizationRequest) => {
      const response = await apiClient.request<CustomizationResponse>(
        `/v1/tenants/${tenantId}/policies/customize`,
        {
          method: 'POST',
          body: JSON.stringify(data),
        }
      );
      return response;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tenantPolicyKeys.list(tenantId) });
      toast.success('Policy customization created');
    },
    onError: (error: Error) => {
      toast.error(`Failed to create customization: ${error.message}`);
    },
  });
}

// Update customization
export function useUpdateCustomization(tenantId: string, customizationId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (data: UpdateCustomizationRequest) => {
      const response = await apiClient.request<CustomizationResponse>(
        `/v1/tenants/${tenantId}/policies/customizations/${customizationId}`,
        {
          method: 'PUT',
          body: JSON.stringify(data),
        }
      );
      return response;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tenantPolicyKeys.list(tenantId) });
      queryClient.invalidateQueries({
        queryKey: tenantPolicyKeys.detail(tenantId, customizationId),
      });
      toast.success('Policy customization updated');
    },
    onError: (error: Error) => {
      toast.error(`Failed to update customization: ${error.message}`);
    },
  });
}

// Delete customization
export function useDeleteCustomization(tenantId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (customizationId: string) => {
      await apiClient.request(
        `/v1/tenants/${tenantId}/policies/customizations/${customizationId}`,
        {
          method: 'DELETE',
        }
      );
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tenantPolicyKeys.list(tenantId) });
      toast.success('Policy customization deleted');
    },
    onError: (error: Error) => {
      toast.error(`Failed to delete customization: ${error.message}`);
    },
  });
}

// Submit for review
export function useSubmitForReview(tenantId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (customizationId: string) => {
      const response = await apiClient.request<CustomizationResponse>(
        `/v1/tenants/${tenantId}/policies/customizations/${customizationId}/submit`,
        {
          method: 'POST',
        }
      );
      return response;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tenantPolicyKeys.list(tenantId) });
      queryClient.invalidateQueries({ queryKey: tenantPolicyKeys.pendingReviews() });
      toast.success('Policy customization submitted for review');
    },
    onError: (error: Error) => {
      toast.error(`Failed to submit for review: ${error.message}`);
    },
  });
}

// Approve customization
export function useApproveCustomization() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ customizationId, notes }: { customizationId: string; notes?: string }) => {
      const response = await apiClient.request<CustomizationResponse>(
        `/v1/policies/customizations/${customizationId}/approve`,
        {
          method: 'POST',
          body: JSON.stringify({ notes }),
        }
      );
      return response;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tenantPolicyKeys.all });
      toast.success('Policy customization approved');
    },
    onError: (error: Error) => {
      toast.error(`Failed to approve customization: ${error.message}`);
    },
  });
}

// Reject customization
export function useRejectCustomization() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ customizationId, notes }: { customizationId: string; notes?: string }) => {
      const response = await apiClient.request<CustomizationResponse>(
        `/v1/policies/customizations/${customizationId}/reject`,
        {
          method: 'POST',
          body: JSON.stringify({ notes }),
        }
      );
      return response;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tenantPolicyKeys.all });
      toast.success('Policy customization rejected');
    },
    onError: (error: Error) => {
      toast.error(`Failed to reject customization: ${error.message}`);
    },
  });
}

// Activate customization
export function useActivateCustomization() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (customizationId: string) => {
      const response = await apiClient.request<CustomizationResponse>(
        `/v1/policies/customizations/${customizationId}/activate`,
        {
          method: 'POST',
        }
      );
      return response;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tenantPolicyKeys.all });
      toast.success('Policy customization activated');
    },
    onError: (error: Error) => {
      toast.error(`Failed to activate customization: ${error.message}`);
    },
  });
}

