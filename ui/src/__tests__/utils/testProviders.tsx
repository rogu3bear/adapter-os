/**
 * Test provider components and utilities
 *
 * Provides wrapper components with all necessary providers for testing hooks and components.
 */

import React from 'react';
import { render, RenderOptions } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { BrowserRouter, MemoryRouter, MemoryRouterProps } from 'react-router-dom';

/**
 * Configuration for test QueryClient
 */
const createTestQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        // Disable retries for faster tests
        retry: false,
        // Disable cache time for isolated tests
        gcTime: 0,
        // Disable refetch behaviors
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
        refetchOnMount: false,
      },
      mutations: {
        retry: false,
      },
    },
    // Suppress console errors in tests
    logger: {
      log: () => {},
      warn: () => {},
      error: () => {},
    },
  });

/**
 * Props for AllProviders wrapper
 */
interface AllProvidersProps {
  children: React.ReactNode;
  queryClient?: QueryClient;
  useMemoryRouter?: boolean;
  routerProps?: Partial<MemoryRouterProps>;
}

/**
 * Wrapper component with all necessary providers for testing
 */
export function AllProviders({
  children,
  queryClient,
  useMemoryRouter = true,
  routerProps,
}: AllProvidersProps) {
  const client = queryClient ?? createTestQueryClient();
  const Router = useMemoryRouter ? MemoryRouter : BrowserRouter;

  return (
    <QueryClientProvider client={client}>
      <Router {...routerProps}>{children}</Router>
    </QueryClientProvider>
  );
}

/**
 * Custom render options extending RTL's RenderOptions
 */
interface CustomRenderOptions extends Omit<RenderOptions, 'wrapper'> {
  queryClient?: QueryClient;
  useMemoryRouter?: boolean;
  routerProps?: Partial<MemoryRouterProps>;
}

/**
 * Custom render function that wraps components with all providers
 *
 * Usage:
 * ```tsx
 * const { getByText } = renderWithProviders(<MyComponent />);
 * ```
 */
export function renderWithProviders(
  ui: React.ReactElement,
  options?: CustomRenderOptions
) {
  const {
    queryClient,
    useMemoryRouter = true,
    routerProps,
    ...renderOptions
  } = options ?? {};

  const client = queryClient ?? createTestQueryClient();

  function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <AllProviders
        queryClient={client}
        useMemoryRouter={useMemoryRouter}
        routerProps={routerProps}
      >
        {children}
      </AllProviders>
    );
  }

  return {
    ...render(ui, { wrapper: Wrapper, ...renderOptions }),
    queryClient: client,
  };
}

/**
 * Wrapper for query-only tests (no router needed)
 */
export function QueryWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = createTestQueryClient();
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

/**
 * Custom render for query-only tests
 *
 * Usage:
 * ```tsx
 * const { result } = renderHookWithQuery(() => useDocuments());
 * ```
 */
export function renderWithQuery(ui: React.ReactElement, options?: RenderOptions) {
  const queryClient = createTestQueryClient();

  function Wrapper({ children }: { children: React.ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  }

  return {
    ...render(ui, { wrapper: Wrapper, ...options }),
    queryClient,
  };
}

/**
 * Helper to wait for all pending queries to settle
 */
export async function waitForQueries(queryClient: QueryClient) {
  await queryClient.cancelQueries();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

/**
 * Helper to clear all query cache
 */
export function clearQueryCache(queryClient: QueryClient) {
  queryClient.clear();
}

/**
 * Mock auth context for testing authenticated components
 */
export const mockAuthContext = {
  isAuthenticated: true,
  user: {
    id: 'test-user',
    email: 'test@example.com',
    role: 'admin' as const,
  },
  tenant: {
    id: 'tenant-1',
    name: 'Test Tenant',
  },
  login: jest.fn(),
  logout: jest.fn(),
  refreshToken: jest.fn(),
};

/**
 * Mock router context for testing navigation
 */
export const mockRouterContext = {
  navigate: jest.fn(),
  location: {
    pathname: '/test',
    search: '',
    hash: '',
    state: null,
    key: 'default',
  },
  params: {},
};

/**
 * Helper to create mock initial router entries
 */
export function createRouterEntries(paths: string[]) {
  return paths.map((path) => ({ pathname: path }));
}

/**
 * Helper to setup a test with initial route
 */
export function renderWithRoute(
  ui: React.ReactElement,
  initialRoute: string = '/',
  options?: CustomRenderOptions
) {
  return renderWithProviders(ui, {
    ...options,
    routerProps: {
      initialEntries: [initialRoute],
      ...options?.routerProps,
    },
  });
}
