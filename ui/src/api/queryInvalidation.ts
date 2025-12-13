import { QueryClient } from '@tanstack/react-query';
import { withTenantKey } from '@/utils/tenant';

type MaybeQueryClient = QueryClient | null | undefined;

const ensureClient = (client?: MaybeQueryClient) => client ?? new QueryClient();

export async function invalidateDashboard(client?: MaybeQueryClient, tenantId?: string | null) {
  const queryClient = ensureClient(client);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['dashboard'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['dashboard'], tenantId) }),
    queryClient.invalidateQueries({ queryKey: ['metrics'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['metrics'], tenantId) }),
    queryClient.invalidateQueries({ queryKey: ['system'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['system'], tenantId) }),
  ]);
}

export async function invalidateTelemetry(client?: MaybeQueryClient, tenantId?: string | null) {
  const queryClient = ensureClient(client);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['telemetry'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['telemetry'], tenantId) }),
    queryClient.invalidateQueries({ queryKey: ['sessions'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['sessions'], tenantId) }),
    queryClient.invalidateQueries({ queryKey: ['chat-sessions'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['chat-sessions'], tenantId) }),
  ]);
}

export async function invalidateModels(client?: MaybeQueryClient, tenantId?: string | null) {
  const queryClient = ensureClient(client);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['models'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['models'], tenantId) }),
    queryClient.invalidateQueries({ queryKey: ['base-models'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['base-models'], tenantId) }),
  ]);
}

export async function invalidateTrainingCaches(client?: MaybeQueryClient, tenantId?: string | null) {
  const queryClient = ensureClient(client);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['training'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['training'], tenantId) }),
    queryClient.invalidateQueries({ queryKey: ['datasets'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['datasets'], tenantId) }),
    queryClient.invalidateQueries({ queryKey: ['jobs'] }),
    queryClient.invalidateQueries({ queryKey: withTenantKey(['jobs'], tenantId) }),
  ]);
}

