import React, { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { CoreProviders } from './CoreProviders';
import { FeatureProviders } from './FeatureProviders';

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

export function AppProviders({ children }: { children: ReactNode }) {
  console.log("AppProviders rendering");
  return (
    <QueryClientProvider client={queryClient}>
      <CoreProviders>
        <FeatureProviders>
          {children}
        </FeatureProviders>
      </CoreProviders>
    </QueryClientProvider>
  );
}

