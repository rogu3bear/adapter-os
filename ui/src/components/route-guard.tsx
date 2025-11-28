import React, { Suspense } from 'react';
import { Navigate } from 'react-router-dom';
import { ErrorBoundary } from 'react-error-boundary';
import { AlertTriangle } from 'lucide-react';
import { useAuth } from '@/providers/CoreProviders';
import type { RouteConfig } from '@/config/routes';
import { canAccessRoute } from '@/config/routes';
import { PageSkeleton } from '@/components/ui/page-skeleton';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';

interface ErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
}

function ErrorFallback({ error, resetErrorBoundary }: ErrorFallbackProps) {
  return (
    <div className="min-h-screen flex items-center justify-center p-4">
      <Card className="max-w-md w-full p-6 text-center">
        <AlertTriangle className="h-12 w-12 text-destructive mx-auto mb-4" />
        <h2 className="text-lg font-semibold mb-2">Failed to load page</h2>
        <p className="text-sm text-muted-foreground mb-4">
          {error.message || 'An unexpected error occurred while loading this page.'}
        </p>
        <Button onClick={resetErrorBoundary}>
          Try Again
        </Button>
      </Card>
    </div>
  );
}

interface RouteGuardProps {
  route: RouteConfig;
  children?: React.ReactNode;
  fallbackPath?: string;
}

/**
 * Unified route wrapper that enforces auth/role requirements
 */
export function RouteGuard({ route, children, fallbackPath = '/dashboard' }: RouteGuardProps) {
  const { user, isLoading } = useAuth();
  const requiresAuth =
    route.requiresAuth !== false ||
    (route.requiredRoles?.length ?? 0) > 0 ||
    (route.requiredPermissions?.length ?? 0) > 0;

  // Show loading state while auth is being verified
  if (requiresAuth && isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div role="status" aria-label="Loading" className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
      </div>
    );
  }

  // If route requires authentication and user is not authenticated
  if (requiresAuth && !user) {
    return <Navigate to="/login" replace />;
  }

  // Check if user has required role
  if (user && !canAccessRoute(route, user.role, user.permissions ?? [])) {
    return <Navigate to={fallbackPath} replace />;
  }

  const Component = route.component;
  if (children) {
    return <>{children}</>;
  }

  return (
    <ErrorBoundary FallbackComponent={ErrorFallback}>
      <Suspense fallback={<PageSkeleton variant={route.skeletonVariant || 'default'} />}>
        <Component />
      </Suspense>
    </ErrorBoundary>
  );
}

/**
 * Create a route guard component for a specific route config
 * Useful for inline route definitions
 */
export function createRouteGuard(route: RouteConfig) {
  return ({ children }: { children: React.ReactNode }) => (
    <RouteGuard route={route} fallbackPath="/dashboard">
      {children}
    </RouteGuard>
  );
}
