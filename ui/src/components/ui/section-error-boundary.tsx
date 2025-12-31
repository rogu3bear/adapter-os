/**
 * @deprecated Use ErrorBoundary from '@/components/ui/error-boundary' with scope="section"
 * @example
 * import { ErrorBoundary } from '@/components/ui/error-boundary';
 * <ErrorBoundary scope="section" name="Settings">...</ErrorBoundary>
 */
import React from 'react';
import { ErrorBoundary, withErrorBoundary, SectionErrorFallback } from './error-boundary';

// Re-export the fallback for tests that reference it directly
export { SectionErrorFallback };

interface SectionErrorBoundaryProps {
  children: React.ReactNode;
  sectionName?: string;
  onReset?: () => void;
  fallback?: React.ReactNode;
  severity?: 'warning' | 'error';
}

/**
 * @deprecated Use ErrorBoundary with scope="section" instead
 */
export function SectionErrorBoundary({
  children,
  sectionName,
  onReset,
  fallback,
  severity = 'error',
}: SectionErrorBoundaryProps) {
  return (
    <ErrorBoundary
      scope="section"
      name={sectionName}
      onReset={onReset}
      fallback={fallback}
      severity={severity}
    >
      {children}
    </ErrorBoundary>
  );
}

/**
 * @deprecated Use withErrorBoundary with scope="section" instead
 */
export function withSectionErrorBoundary<P extends object>(
  Component: React.ComponentType<P>,
  sectionName: string
) {
  return withErrorBoundary(Component, { scope: 'section', name: sectionName });
}
