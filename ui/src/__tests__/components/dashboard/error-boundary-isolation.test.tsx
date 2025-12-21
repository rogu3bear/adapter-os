/**
 * Tests for dashboard error boundary isolation
 *
 * Verifies that errors in one dashboard widget don't crash
 * other widgets or the entire dashboard.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { SectionErrorBoundary, withSectionErrorBoundary } from '@/components/ui/section-error-boundary';

// Mock logUIError
vi.mock('@/lib/logUIError', () => ({
  logUIError: vi.fn(),
}));

// Suppress console.error for expected errors
const originalError = console.error;
beforeEach(() => {
  console.error = vi.fn();
});
afterEach(() => {
  console.error = originalError;
});

// Component that throws on demand
function ThrowingComponent({ shouldThrow, label }: { shouldThrow: boolean; label: string }) {
  if (shouldThrow) {
    throw new Error(`Error in ${label}`);
  }
  return <div data-testid={`widget-${label}`}>{label} content</div>;
}

// Component that throws on render
function AlwaysThrows({ label }: { label: string }) {
  throw new Error(`${label} always throws`);
}

describe('SectionErrorBoundary', () => {
  it('renders children when no error', () => {
    render(
      <SectionErrorBoundary sectionName="Test Section">
        <div data-testid="child">Hello</div>
      </SectionErrorBoundary>
    );

    expect(screen.getByTestId('child')).toBeInTheDocument();
    expect(screen.getByText('Hello')).toBeInTheDocument();
  });

  it('catches error and shows fallback UI', () => {
    render(
      <SectionErrorBoundary sectionName="Failing Section">
        <AlwaysThrows label="test" />
      </SectionErrorBoundary>
    );

    // Should show error UI, not crash
    expect(screen.getByText(/Failing Section failed to load/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /retry loading this section/i })).toBeInTheDocument();
  });

  it('shows section name in error message', () => {
    render(
      <SectionErrorBoundary sectionName="Custom Widget">
        <AlwaysThrows label="test" />
      </SectionErrorBoundary>
    );

    expect(screen.getByText(/Custom Widget failed to load/i)).toBeInTheDocument();
  });

  it('provides retry button that resets error state', async () => {
    // Use a ref-like approach that survives React's double-rendering
    const throwState = { shouldThrow: true };

    function ConditionalThrow() {
      if (throwState.shouldThrow) {
        throw new Error('Conditional throw');
      }
      return <div data-testid="recovered">Recovered!</div>;
    }

    render(
      <SectionErrorBoundary sectionName="Retry Test">
        <ConditionalThrow />
      </SectionErrorBoundary>
    );

    // First render shows error
    expect(screen.getByText(/Retry Test failed to load/i)).toBeInTheDocument();

    // Set to not throw on next render
    throwState.shouldThrow = false;

    // Click retry
    const retryButton = screen.getByRole('button', { name: /retry loading this section/i });
    retryButton.click();

    // After retry, should show recovered content
    expect(await screen.findByTestId('recovered')).toBeInTheDocument();
  });
});

describe('withSectionErrorBoundary HOC', () => {
  it('wraps component with error boundary', () => {
    function MyWidget() {
      return <div data-testid="my-widget">Widget Content</div>;
    }
    const WrappedWidget = withSectionErrorBoundary(MyWidget, 'My Widget');

    render(<WrappedWidget />);

    expect(screen.getByTestId('my-widget')).toBeInTheDocument();
  });

  it('catches errors from wrapped component', () => {
    function FailingWidget() {
      throw new Error('Widget failed');
    }
    const WrappedWidget = withSectionErrorBoundary(FailingWidget, 'Failing Widget');

    render(<WrappedWidget />);

    expect(screen.getByText(/Failing Widget failed to load/i)).toBeInTheDocument();
  });

  it('passes props through to wrapped component', () => {
    function PropsWidget({ message }: { message: string }) {
      return <div data-testid="props-widget">{message}</div>;
    }
    const WrappedWidget = withSectionErrorBoundary(PropsWidget, 'Props Widget');

    render(<WrappedWidget message="Hello from props" />);

    expect(screen.getByText('Hello from props')).toBeInTheDocument();
  });
});

describe('Error isolation between widgets', () => {
  it('error in one widget does not affect siblings', () => {
    render(
      <div>
        <SectionErrorBoundary sectionName="Widget A">
          <div data-testid="widget-a">Widget A content</div>
        </SectionErrorBoundary>

        <SectionErrorBoundary sectionName="Widget B">
          <AlwaysThrows label="B" />
        </SectionErrorBoundary>

        <SectionErrorBoundary sectionName="Widget C">
          <div data-testid="widget-c">Widget C content</div>
        </SectionErrorBoundary>
      </div>
    );

    // Widget A should be visible
    expect(screen.getByTestId('widget-a')).toBeInTheDocument();

    // Widget B shows error
    expect(screen.getByText(/Widget B failed to load/i)).toBeInTheDocument();

    // Widget C should still be visible
    expect(screen.getByTestId('widget-c')).toBeInTheDocument();
  });

  it('multiple widgets can fail independently', () => {
    render(
      <div>
        <SectionErrorBoundary sectionName="Success">
          <div data-testid="success">OK</div>
        </SectionErrorBoundary>

        <SectionErrorBoundary sectionName="Fail 1">
          <AlwaysThrows label="1" />
        </SectionErrorBoundary>

        <SectionErrorBoundary sectionName="Fail 2">
          <AlwaysThrows label="2" />
        </SectionErrorBoundary>
      </div>
    );

    // Success widget works
    expect(screen.getByTestId('success')).toBeInTheDocument();

    // Both failures are contained
    expect(screen.getByText(/Fail 1 failed to load/i)).toBeInTheDocument();
    expect(screen.getByText(/Fail 2 failed to load/i)).toBeInTheDocument();

    // Two retry buttons (one per failed widget)
    const retryButtons = screen.getAllByRole('button', { name: /retry loading this section/i });
    expect(retryButtons).toHaveLength(2);
  });

  it('error in nested component is caught by nearest boundary', () => {
    render(
      <SectionErrorBoundary sectionName="Outer">
        <div data-testid="outer-content">Outer content</div>
        <SectionErrorBoundary sectionName="Inner">
          <AlwaysThrows label="nested" />
        </SectionErrorBoundary>
      </SectionErrorBoundary>
    );

    // Outer content should be visible
    expect(screen.getByTestId('outer-content')).toBeInTheDocument();

    // Inner boundary catches the error, not outer
    expect(screen.getByText(/Inner failed to load/i)).toBeInTheDocument();
    expect(screen.queryByText(/Outer failed to load/i)).not.toBeInTheDocument();
  });
});

describe('Dashboard widget error scenarios', () => {
  it('simulates SSE hook error being contained', () => {
    // Simulate a widget that uses SSE and fails
    function SSEWidget() {
      // Simulate SSE hook throwing
      throw new Error('SSE connection failed');
    }

    render(
      <div>
        <SectionErrorBoundary sectionName="Metrics">
          <SSEWidget />
        </SectionErrorBoundary>

        <SectionErrorBoundary sectionName="Static Content">
          <div data-testid="static">This is static content</div>
        </SectionErrorBoundary>
      </div>
    );

    // SSE widget shows error
    expect(screen.getByText(/Metrics failed to load/i)).toBeInTheDocument();

    // Static content is unaffected
    expect(screen.getByTestId('static')).toBeInTheDocument();
  });

  it('simulates data fetch error being contained', () => {
    function DataFetchWidget() {
      throw new Error('Failed to fetch data');
    }

    render(
      <div>
        <SectionErrorBoundary sectionName="Activity Feed">
          <DataFetchWidget />
        </SectionErrorBoundary>

        <SectionErrorBoundary sectionName="Alerts">
          <div data-testid="alerts">No active alerts</div>
        </SectionErrorBoundary>
      </div>
    );

    expect(screen.getByText(/Activity Feed failed to load/i)).toBeInTheDocument();
    expect(screen.getByTestId('alerts')).toBeInTheDocument();
  });
});
