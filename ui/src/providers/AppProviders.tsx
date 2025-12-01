import React, { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { CoreProviders } from './CoreProviders';
import { FeatureProviders } from './FeatureProviders';
import { PersistentNotificationProvider } from '@/components/PersistentNotifications';
import { ErrorStoreProvider } from '@/stores/errorStore';

// Create a QueryClient instance with default options
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
      staleTime: 5 * 60 * 1000, // 5 minutes
    },
  },
});

// Wrapper that conditionally includes dev-only providers
function DevProviders({ children }: { children: ReactNode }) {
  if (import.meta.env.DEV) {
    return <ErrorStoreProvider>{children}</ErrorStoreProvider>;
  }
  return <>{children}</>;
}

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <CoreProviders>
        <FeatureProviders>
          <PersistentNotificationProvider>
            <DevProviders>
              {children}
            </DevProviders>
          </PersistentNotificationProvider>
        </FeatureProviders>
      </CoreProviders>
    </QueryClientProvider>
  );
}

