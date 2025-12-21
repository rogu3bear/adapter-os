import React, { ReactNode, useState } from 'react';
import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { CoreProviders } from './CoreProviders';
import { FeatureProviders } from './FeatureProviders';
import { PersistentNotificationProvider } from '@/components/PersistentNotifications';
import { ErrorStoreProvider } from '@/stores/errorStore';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ToastProvider } from '@/components/toast/ToastProvider';
import { createQueryErrorHandler } from '@/lib/queryErrorHandler';
import { QUERY_STANDARD, queryClientOptions } from '@/api/queryOptions';

// Create a QueryClient instance with default options and centralized error handling
function buildQueryClient() {
  const handleQueryError = createQueryErrorHandler();

  return new QueryClient({
    ...queryClientOptions,
    queryCache: new QueryCache({
      onError: handleQueryError,
    }),
    mutationCache: new MutationCache({
      onError: handleQueryError,
    }),
    defaultOptions: {
      ...queryClientOptions.defaultOptions,
      queries: {
        ...QUERY_STANDARD,
        ...queryClientOptions.defaultOptions?.queries,
      },
    },
  });
}

// Wrapper that conditionally includes dev-only providers
function DevProviders({ children }: { children: ReactNode }) {
  if (import.meta.env.DEV) {
    return <ErrorStoreProvider>{children}</ErrorStoreProvider>;
  }
  return <>{children}</>;
}

export function AppProviders({ children }: { children: ReactNode }) {
  const [queryClient] = useState(() => buildQueryClient());

  return (
    <QueryClientProvider client={queryClient}>
      <CoreProviders>
        <FeatureProviders>
          <TooltipProvider delayDuration={0}>
            <PersistentNotificationProvider>
              <ToastProvider>
                <DevProviders>
                  {children}
                </DevProviders>
              </ToastProvider>
            </PersistentNotificationProvider>
          </TooltipProvider>
        </FeatureProviders>
      </CoreProviders>
    </QueryClientProvider>
  );
}

