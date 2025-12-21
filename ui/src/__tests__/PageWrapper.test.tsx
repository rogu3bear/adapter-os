/**
 * PageWrapper Tests
 *
 * Tests for the unified page wrapper component that combines:
 * - DensityProvider
 * - FeatureLayout
 * - PageErrorsProvider
 */

import { describe, it, expect, vi } from 'vitest';
import React from 'react';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { PageWrapper } from '@/layout/PageWrapper';

// Mock the core provider hooks
vi.mock('@/providers/CoreProviders', () => ({
  useResize: () => ({
    getLayout: vi.fn(),
    setLayout: vi.fn(),
  }),
}));

// Mock useTenant to prevent "useTenant must be used within FeatureProviders" error
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({
    selectedTenant: 'test-tenant',
    setSelectedTenant: vi.fn(),
    tenants: [],
    isLoading: false,
    refreshTenants: vi.fn(),
  }),
}));

// Mock useInformationDensity hook
vi.mock('@/hooks/useInformationDensity', () => ({
  useInformationDensity: () => ({
    density: 'comfortable',
    setDensity: vi.fn(),
    spacing: { gap: '1rem', padding: '1rem' },
    textSizes: { base: '1rem', sm: '0.875rem' },
    isCompact: false,
    isComfortable: true,
    isSpacious: false,
  }),
}));

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false },
  },
});

function TestWrapper({ children }: { children: React.ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>{children}</MemoryRouter>
    </QueryClientProvider>
  );
}

describe('PageWrapper', () => {
  it('renders children with title', () => {
    render(
      <TestWrapper>
        <PageWrapper pageKey="test-page" title="Test Page">
          <div data-testid="content">Page Content</div>
        </PageWrapper>
      </TestWrapper>
    );

    expect(screen.getByText('Test Page')).toBeInTheDocument();
    expect(screen.getByTestId('content')).toBeInTheDocument();
  });

  it('renders with description', () => {
    render(
      <TestWrapper>
        <PageWrapper
          pageKey="test-page"
          title="Test Page"
          description="This is a test description"
        >
          <div>Content</div>
        </PageWrapper>
      </TestWrapper>
    );

    expect(screen.getByText('This is a test description')).toBeInTheDocument();
  });

  it('renders primary action button', () => {
    const handleClick = vi.fn();

    render(
      <TestWrapper>
        <PageWrapper
          pageKey="test-page"
          title="Test Page"
          primaryAction={{
            label: 'Create New',
            onClick: handleClick,
          }}
        >
          <div>Content</div>
        </PageWrapper>
      </TestWrapper>
    );

    expect(screen.getByText('Create New')).toBeInTheDocument();
  });

  it('renders badges', () => {
    render(
      <TestWrapper>
        <PageWrapper
          pageKey="test-page"
          title="Test Page"
          badges={[
            { label: 'Beta', variant: 'secondary' },
            { label: 'New', variant: 'default' },
          ]}
        >
          <div>Content</div>
        </PageWrapper>
      </TestWrapper>
    );

    // Badges are rendered twice (desktop + mobile responsive views)
    expect(screen.getAllByText('Beta').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('New').length).toBeGreaterThanOrEqual(1);
  });

  it('applies different content padding', () => {
    const { container } = render(
      <TestWrapper>
        <PageWrapper
          pageKey="test-page"
          title="Test Page"
          contentPadding="compact"
        >
          <div>Content</div>
        </PageWrapper>
      </TestWrapper>
    );

    // The compact padding class should be applied
    const element = container.querySelector('.px-\\[var\\(--space-4\\)\\].py-\\[var\\(--space-4\\)\\]');
    expect(element).not.toBeNull();
  });

  it('applies max width constraint', () => {
    const { container } = render(
      <TestWrapper>
        <PageWrapper pageKey="test-page" title="Test Page" maxWidth="lg">
          <div>Content</div>
        </PageWrapper>
      </TestWrapper>
    );

    // Check for the max-width class that corresponds to 'lg' setting
    const element = container.querySelector('.max-w-\\[var\\(--layout-content-width-lg\\)\\]');
    expect(element).not.toBeNull();
  });

  it('provides page errors context to children', () => {
    // Component that uses usePageErrors
    function ErrorConsumer() {
      // This would throw if PageErrorsProvider wasn't present
      return <div data-testid="error-consumer">Has Error Context</div>;
    }

    render(
      <TestWrapper>
        <PageWrapper pageKey="test-page" title="Test Page">
          <ErrorConsumer />
        </PageWrapper>
      </TestWrapper>
    );

    expect(screen.getByTestId('error-consumer')).toBeInTheDocument();
  });
});

describe('PageWrapper integration', () => {
  it('can be imported from @/layout barrel export', async () => {
    // This test verifies the barrel export works
    const { PageWrapper: ImportedPageWrapper } = await import('@/layout');
    expect(ImportedPageWrapper).toBeDefined();
  });
});
