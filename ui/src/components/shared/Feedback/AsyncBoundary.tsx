"use client";

import React, { Component, ErrorInfo, ReactNode, Suspense } from "react";
import { ErrorBoundary } from "./ErrorBoundary";
import { LoadingState } from "@/components/ui/loading-state";
import { ErrorRecovery } from "@/components/ui/error-recovery";
import { logUIError } from "@/lib/logUIError";

export interface AsyncBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
  errorFallback?: ReactNode | ((error: Error, resetError: () => void) => ReactNode);
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
  onReset?: () => void;
  sectionName?: string;
}

/**
 * AsyncBoundary - Combines ErrorBoundary + Suspense for async components
 * Provides unified error handling and loading states for components that fetch data
 */
export function AsyncBoundary({
  children,
  fallback,
  errorFallback,
  onError,
  onReset,
  sectionName = "section",
}: AsyncBoundaryProps) {
  const handleError = (error: Error, errorInfo: ErrorInfo) => {
    console.error(`[AsyncBoundary:${sectionName}] Error caught:`, error);
    console.error(`[AsyncBoundary:${sectionName}] Component stack:`, errorInfo.componentStack);

    // Log to error tracking
    logUIError(error, {
      scope: 'async-boundary',
      component: sectionName,
      severity: 'error',
    });

    // Call custom error handler if provided
    onError?.(error, errorInfo);
  };

  const defaultErrorFallback = (error: Error, resetError: () => void) => (
    <div className="p-4">
      <ErrorRecovery
        error={`Failed to load ${sectionName}: ${error.message}`}
        onRetry={resetError}
      />
    </div>
  );

  const getErrorFallback = () => {
    if (!errorFallback) {
      return ({ error, resetError }: { error: Error; resetError: () => void }) =>
        defaultErrorFallback(error, resetError);
    }

    if (typeof errorFallback === "function") {
      return ({ error, resetError }: { error: Error; resetError: () => void }) =>
        errorFallback(error, resetError);
    }

    return () => errorFallback;
  };

  return (
    <ErrorBoundary
      fallback={getErrorFallback()}
      onError={handleError}
      onReset={onReset}
    >
      <Suspense fallback={fallback || <LoadingState />}>
        {children}
      </Suspense>
    </ErrorBoundary>
  );
}

export interface PageAsyncBoundaryProps {
  children: ReactNode;
  pageName?: string;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
  onReset?: () => void;
}

/**
 * PageAsyncBoundary - Error boundary wrapper for full pages
 * Provides page-level error handling with skeleton loading and full-page error states
 */
export function PageAsyncBoundary({
  children,
  pageName = "page",
  onError,
  onReset,
}: PageAsyncBoundaryProps) {
  const handleError = (error: Error, errorInfo: ErrorInfo) => {
    console.error(`[PageAsyncBoundary:${pageName}] Error caught:`, error);
    console.error(`[PageAsyncBoundary:${pageName}] Component stack:`, errorInfo.componentStack);

    // Log to error tracking
    logUIError(error, {
      scope: 'page',
      component: pageName,
      severity: 'error',
    });

    // Call custom error handler if provided
    onError?.(error, errorInfo);
  };

  const pageErrorFallback = (error: Error, resetError: () => void) => (
    <div className="container mx-auto px-4 py-8 max-w-2xl">
      <ErrorRecovery
        error={`Unable to load ${pageName}: ${error.message}`}
        onRetry={resetError}
      />
    </div>
  );

  const pageLoadingFallback = (
    <div className="container mx-auto px-4 py-8">
      <LoadingState
        title={`Loading ${pageName}...`}
        description="Fetching data"
        skeletonLines={8}
      />
    </div>
  );

  return (
    <ErrorBoundary
      fallback={({ error, resetError }) => pageErrorFallback(error, resetError)}
      onError={handleError}
      onReset={onReset}
    >
      <Suspense fallback={pageLoadingFallback}>
        {children}
      </Suspense>
    </ErrorBoundary>
  );
}

export interface SectionAsyncBoundaryProps {
  children: ReactNode;
  section: string;
  fallback?: ReactNode;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
  onReset?: () => void;
}

/**
 * SectionAsyncBoundary - Error boundary wrapper for page sections
 * Provides section-level error handling with compact error states
 * Prevents section errors from crashing the entire page
 */
export function SectionAsyncBoundary({
  children,
  section,
  fallback,
  onError,
  onReset,
}: SectionAsyncBoundaryProps) {
  const handleError = (error: Error, errorInfo: ErrorInfo) => {
    console.error(`[SectionAsyncBoundary:${section}] Error caught:`, error);
    console.error(`[SectionAsyncBoundary:${section}] Component stack:`, errorInfo.componentStack);

    // Log to error tracking with section context
    logUIError(error, {
      scope: 'section',
      component: section,
      severity: 'warning',
    });

    // Call custom error handler if provided
    onError?.(error, errorInfo);
  };

  const sectionErrorFallback = (error: Error, resetError: () => void) => (
    <div className="p-3 border border-destructive/20 rounded-md bg-destructive/5">
      <div className="text-sm text-destructive mb-2">
        Failed to load {section}
      </div>
      <ErrorRecovery
        error={error.message}
        onRetry={resetError}
      />
    </div>
  );

  const sectionLoadingFallback = fallback || (
    <div className="p-3">
      <LoadingState message={`Loading ${section}...`} />
    </div>
  );

  return (
    <ErrorBoundary
      fallback={({ error, resetError }) => sectionErrorFallback(error, resetError)}
      onError={handleError}
      onReset={onReset}
    >
      <Suspense fallback={sectionLoadingFallback}>
        {children}
      </Suspense>
    </ErrorBoundary>
  );
}

export default AsyncBoundary;
