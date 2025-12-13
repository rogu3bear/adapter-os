import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import apiClient from '@/api/client';
import type {
  Tenant,
  CreateTenantRequest,
  TenantUsageResponse,
  AssignPoliciesResponse,
  AssignAdaptersResponse,
  AdapterStack,
  CreateAdapterStackRequest,
  User,
  UserRole,
  RegisterUserRequest,
  UpdateUserRequest,
  ListUsersResponse,
} from '@/api/types';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';

// Tenants
export function useTenants() {
  return useQuery({
    queryKey: ['tenants'],
    queryFn: () => apiClient.listTenants(),
    staleTime: 30000,
  });
}

export function useCreateTenant() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateTenantRequest) => apiClient.createTenant(data),
    onSuccess: (newTenant) => {
      queryClient.invalidateQueries({ queryKey: ['tenants'] });
      toast.success(`Organization "${newTenant.name}" created successfully`);
      logger.info('Tenant created', {
        component: 'useAdmin',
        operation: 'createTenant',
        tenantId: newTenant.id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to create organization: ${error.message}`);
      logger.error('Failed to create tenant', {
        component: 'useAdmin',
        operation: 'createTenant',
      }, error);
    },
  });
}

export function useUpdateTenant() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ tenantId, name }: { tenantId: string; name: string }) =>
      apiClient.updateTenant(tenantId, name),
    onSuccess: (updatedTenant) => {
      queryClient.invalidateQueries({ queryKey: ['tenants'] });
      toast.success(`Organization "${updatedTenant.name}" updated successfully`);
      logger.info('Tenant updated', {
        component: 'useAdmin',
        operation: 'updateTenant',
        tenantId: updatedTenant.id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to update organization: ${error.message}`);
      logger.error('Failed to update tenant', {
        component: 'useAdmin',
        operation: 'updateTenant',
      }, error);
    },
  });
}

export function usePauseTenant() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (tenantId: string) => apiClient.pauseTenant(tenantId),
    onSuccess: (_, tenantId) => {
      queryClient.invalidateQueries({ queryKey: ['tenants'] });
      toast.success('Organization paused successfully');
      logger.info('Tenant paused', {
        component: 'useAdmin',
        operation: 'pauseTenant',
        tenantId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to pause organization: ${error.message}`);
      logger.error('Failed to pause tenant', {
        component: 'useAdmin',
        operation: 'pauseTenant',
      }, error);
    },
  });
}

export function useArchiveTenant() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (tenantId: string) => apiClient.archiveTenant(tenantId),
    onSuccess: (_, tenantId) => {
      queryClient.invalidateQueries({ queryKey: ['tenants'] });
      toast.success('Organization archived successfully');
      logger.info('Tenant archived', {
        component: 'useAdmin',
        operation: 'archiveTenant',
        tenantId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to archive organization: ${error.message}`);
      logger.error('Failed to archive tenant', {
        component: 'useAdmin',
        operation: 'archiveTenant',
      }, error);
    },
  });
}

export function useTenantUsage(tenantId: string) {
  return useQuery({
    queryKey: ['tenant-usage', tenantId],
    queryFn: () => apiClient.getTenantUsage(tenantId),
    enabled: !!tenantId,
  });
}

export function useAssignTenantPolicies() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ tenantId, cpids }: { tenantId: string; cpids: string[] }) =>
      apiClient.assignTenantPolicies(tenantId, cpids),
    onSuccess: (_, { tenantId }) => {
      queryClient.invalidateQueries({ queryKey: ['tenants'] });
      toast.success('Policies assigned successfully');
      logger.info('Policies assigned to tenant', {
        component: 'useAdmin',
        operation: 'assignPolicies',
        tenantId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to assign policies: ${error.message}`);
      logger.error('Failed to assign policies', {
        component: 'useAdmin',
        operation: 'assignPolicies',
      }, error);
    },
  });
}

export function useAssignTenantAdapters() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ tenantId, adapterIds }: { tenantId: string; adapterIds: string[] }) =>
      apiClient.assignTenantAdapters(tenantId, adapterIds),
    onSuccess: (_, { tenantId }) => {
      queryClient.invalidateQueries({ queryKey: ['tenants'] });
      toast.success('Adapters assigned successfully');
      logger.info('Adapters assigned to tenant', {
        component: 'useAdmin',
        operation: 'assignAdapters',
        tenantId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to assign adapters: ${error.message}`);
      logger.error('Failed to assign adapters', {
        component: 'useAdmin',
        operation: 'assignAdapters',
      }, error);
    },
  });
}

// Adapter Stacks
export function useAdapterStacks() {
  return useQuery({
    queryKey: ['adapter-stacks'],
    queryFn: () => apiClient.listAdapterStacks(),
    staleTime: 30000,
  });
}

export function useCreateAdapterStack() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateAdapterStackRequest) => apiClient.createAdapterStack(data),
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });
      // Only show success toast if no warnings (warnings will be shown in UI)
      if (!response.warnings || response.warnings.length === 0) {
        toast.success(`Adapter stack "${response.stack.name}" created successfully`);
      }
      logger.info('Adapter stack created', {
        component: 'useAdmin',
        operation: 'createAdapterStack',
        stackId: response.stack.id,
        warnings: response.warnings?.length || 0,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to create adapter stack: ${error.message}`);
      logger.error('Failed to create adapter stack', {
        component: 'useAdmin',
        operation: 'createAdapterStack',
      }, error);
    },
  });
}

export function useUpdateAdapterStack() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ stackId, data }: { stackId: string; data: { name?: string; description?: string; adapters?: Array<{ adapter_id: string; gate: number }> } }) =>
      apiClient.updateAdapterStack(stackId, data),
    onSuccess: (updatedStack) => {
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });
      toast.success(`Stack "${updatedStack.name}" updated`);
      logger.info('Adapter stack updated', {
        component: 'useAdmin',
        operation: 'updateAdapterStack',
        stackId: updatedStack.id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to update stack: ${error.message}`);
      logger.error('Failed to update adapter stack', {
        component: 'useAdmin',
        operation: 'updateAdapterStack',
      }, error);
    },
  });
}

export function useDeleteAdapterStack() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (stackId: string) => apiClient.deleteAdapterStack(stackId),
    onSuccess: (_, stackId) => {
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });
      toast.success('Adapter stack deleted successfully');
      logger.info('Adapter stack deleted', {
        component: 'useAdmin',
        operation: 'deleteAdapterStack',
        stackId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to delete adapter stack: ${error.message}`);
      logger.error('Failed to delete adapter stack', {
        component: 'useAdmin',
        operation: 'deleteAdapterStack',
      }, error);
    },
  });
}

export function useActivateAdapterStack() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (stackId: string) => apiClient.activateAdapterStack(stackId),
    onSuccess: (stack) => {
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });
      toast.success(`Adapter stack "${stack.name}" activated`);
      logger.info('Adapter stack activated', {
        component: 'useAdmin',
        operation: 'activateAdapterStack',
        stackId: stack.id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to activate adapter stack: ${error.message}`);
      logger.error('Failed to activate adapter stack', {
        component: 'useAdmin',
        operation: 'activateAdapterStack',
      }, error);
    },
  });
}

export function useDeactivateAdapterStack() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => apiClient.deactivateAdapterStack(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });
      toast.success('Adapter stack deactivated');
      logger.info('Adapter stack deactivated', {
        component: 'useAdmin',
        operation: 'deactivateAdapterStack',
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to deactivate adapter stack: ${error.message}`);
      logger.error('Failed to deactivate adapter stack', {
        component: 'useAdmin',
        operation: 'deactivateAdapterStack',
      }, error);
    },
  });
}

export function useClearStackAdapters() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (stackId: string) => apiClient.clearStackAdapters(stackId),
    onSuccess: (response) => {
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });
      toast.success(`Cleared ${response.previous_adapter_count} adapter(s) from stack`);
      logger.info('Stack adapters cleared', {
        component: 'useAdmin',
        operation: 'clearStackAdapters',
        stackId: response.stack_id,
        previousCount: response.previous_adapter_count,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to clear adapters: ${error.message}`);
      logger.error('Failed to clear stack adapters', {
        component: 'useAdmin',
        operation: 'clearStackAdapters',
      }, error);
    },
  });
}

export function useGetDefaultStack(tenantId: string | undefined) {
  return useQuery({
    queryKey: ['default-stack', tenantId],
    queryFn: () => apiClient.getDefaultAdapterStack(tenantId!),
    staleTime: 30000,
    enabled: !!tenantId, // Only fetch when we have a real tenant
  });
}

export function useSetDefaultStack(tenantId: string | undefined) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (stackId: string) => apiClient.setDefaultAdapterStack(stackId, tenantId!),
    onSuccess: (_, stackId) => {
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });
      queryClient.invalidateQueries({ queryKey: ['default-stack', tenantId] });
      toast.success('Default stack set successfully');
      logger.info('Default stack set', {
        component: 'useAdmin',
        operation: 'setDefaultStack',
        stackId,
        tenantId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to set default stack: ${error.message}`);
      logger.error('Failed to set default stack', {
        component: 'useAdmin',
        operation: 'setDefaultStack',
      }, error);
    },
  });
}

export function useClearDefaultStack(tenantId: string | undefined) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => apiClient.clearDefaultAdapterStack(tenantId!),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['adapter-stacks'] });
      queryClient.invalidateQueries({ queryKey: ['default-stack', tenantId] });
      toast.success('Default stack cleared');
      logger.info('Default stack cleared', {
        component: 'useAdmin',
        operation: 'clearDefaultStack',
        tenantId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to clear default stack: ${error.message}`);
      logger.error('Failed to clear default stack', {
        component: 'useAdmin',
        operation: 'clearDefaultStack',
      }, error);
    },
  });
}

// Users
export interface UseUsersParams {
  page?: number;
  page_size?: number;
  role?: UserRole;
  tenant_id?: string;
}

export function useUsers(params?: UseUsersParams) {
  return useQuery({
    queryKey: ['users', params],
    queryFn: () => apiClient.listUsers(params),
    staleTime: 30000,
  });
}

export function useUser(userId: string) {
  return useQuery({
    queryKey: ['user', userId],
    queryFn: () => apiClient.getUser(userId),
    enabled: !!userId,
  });
}

export function useCreateUser() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: RegisterUserRequest) => apiClient.createUser(data),
    onSuccess: (newUser) => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      toast.success(`User "${newUser.email}" created successfully`);
      logger.info('User created', {
        component: 'useAdmin',
        operation: 'createUser',
        userId: newUser.user_id || newUser.id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to create user: ${error.message}`);
      logger.error('Failed to create user', {
        component: 'useAdmin',
        operation: 'createUser',
      }, error);
    },
  });
}

export function useUpdateUser() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ userId, data }: { userId: string; data: UpdateUserRequest }) =>
      apiClient.updateUser(userId, data),
    onSuccess: (updatedUser) => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      queryClient.invalidateQueries({ queryKey: ['user', updatedUser.user_id || updatedUser.id] });
      toast.success(`User "${updatedUser.email}" updated successfully`);
      logger.info('User updated', {
        component: 'useAdmin',
        operation: 'updateUser',
        userId: updatedUser.user_id || updatedUser.id,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to update user: ${error.message}`);
      logger.error('Failed to update user', {
        component: 'useAdmin',
        operation: 'updateUser',
      }, error);
    },
  });
}

export function useDeleteUser() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (userId: string) => apiClient.deleteUser(userId),
    onSuccess: (_, userId) => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      queryClient.removeQueries({ queryKey: ['user', userId] });
      toast.success('User deleted successfully');
      logger.info('User deleted', {
        component: 'useAdmin',
        operation: 'deleteUser',
        userId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to delete user: ${error.message}`);
      logger.error('Failed to delete user', {
        component: 'useAdmin',
        operation: 'deleteUser',
      }, error);
    },
  });
}

export function useAssignUserRole() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: UserRole }) =>
      apiClient.assignUserRole(userId, role),
    onSuccess: (updatedUser, { role }) => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      queryClient.invalidateQueries({ queryKey: ['user', updatedUser.user_id || updatedUser.id] });
      toast.success(`Role "${role}" assigned successfully`);
      logger.info('User role assigned', {
        component: 'useAdmin',
        operation: 'assignUserRole',
        userId: updatedUser.user_id || updatedUser.id,
        role,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to assign role: ${error.message}`);
      logger.error('Failed to assign role', {
        component: 'useAdmin',
        operation: 'assignUserRole',
      }, error);
    },
  });
}

export function useResetUserPassword() {
  return useMutation({
    mutationFn: (userId: string) => apiClient.resetUserPassword(userId),
    onSuccess: (_, userId) => {
      toast.success('Password reset email sent');
      logger.info('Password reset initiated', {
        component: 'useAdmin',
        operation: 'resetUserPassword',
        userId,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to reset password: ${error.message}`);
      logger.error('Failed to reset password', {
        component: 'useAdmin',
        operation: 'resetUserPassword',
      }, error);
    },
  });
}

export function useSetUserActive() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ userId, isActive }: { userId: string; isActive: boolean }) =>
      apiClient.setUserActive(userId, isActive),
    onSuccess: (updatedUser, { isActive }) => {
      queryClient.invalidateQueries({ queryKey: ['users'] });
      queryClient.invalidateQueries({ queryKey: ['user', updatedUser.user_id || updatedUser.id] });
      toast.success(`User ${isActive ? 'activated' : 'deactivated'} successfully`);
      logger.info('User active status changed', {
        component: 'useAdmin',
        operation: 'setUserActive',
        userId: updatedUser.user_id || updatedUser.id,
        isActive,
      });
    },
    onError: (error: Error) => {
      toast.error(`Failed to update user status: ${error.message}`);
      logger.error('Failed to update user status', {
        component: 'useAdmin',
        operation: 'setUserActive',
      }, error);
    },
  });
}
