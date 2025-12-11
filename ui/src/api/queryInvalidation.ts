import { QueryClient } from '@tanstack/react-query';

type MaybeQueryClient = QueryClient | null | undefined;

const ensureClient = (client?: MaybeQueryClient) => client ?? new QueryClient();

export async function invalidateDashboard(client?: MaybeQueryClient) {
  const queryClient = ensureClient(client);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['dashboard'] }),
    queryClient.invalidateQueries({ queryKey: ['metrics'] }),
    queryClient.invalidateQueries({ queryKey: ['system'] }),
  ]);
}

export async function invalidateTelemetry(client?: MaybeQueryClient) {
  const queryClient = ensureClient(client);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['telemetry'] }),
    queryClient.invalidateQueries({ queryKey: ['sessions'] }),
    queryClient.invalidateQueries({ queryKey: ['chat-sessions'] }),
  ]);
}

export async function invalidateModels(client?: MaybeQueryClient) {
  const queryClient = ensureClient(client);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['models'] }),
    queryClient.invalidateQueries({ queryKey: ['base-models'] }),
  ]);
}

export async function invalidateTrainingCaches(client?: MaybeQueryClient) {
  const queryClient = ensureClient(client);
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['training'] }),
    queryClient.invalidateQueries({ queryKey: ['datasets'] }),
    queryClient.invalidateQueries({ queryKey: ['jobs'] }),
  ]);
}

