import React, { Suspense, useEffect } from 'react';
import { Navigate } from 'react-router-dom';
import { ErrorBoundary } from 'react-error-boundary';
import { AlertTriangle } from 'lucide-react';
import { useAuth } from '@/providers/CoreProviders';
import type { RouteConfig } from '@/config/routes';
import { canAccessRoute } from '@/config/routes';
import { PageSkeleton } from '@/components/ui/page-skeleton';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import ServerErrorPage from '@/pages/ServerErrorPage';
import { logUIError } from '@/lib/logUIError';
import { TenantRequiredGate } from '@/components/TenantRequiredGate';
import { AuthTimeoutError } from '@/components/ui/auth-timeout';
import {
  validateRouteableAtRuntime,
  isBlockedRouteComponent,
  BLOCKED_ROUTE_COMPONENTS,
} from '@/config/route-types';

interface ErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
}

function ErrorFallback({ error, resetErrorBoundary }: ErrorFallbackProps) {
  return <ServerErrorPage onRetry={resetErrorBoundary} error={error} />;
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
  const { user, isLoading, authTimeout } = useAuth();
  const requiresAuth =
    route.requiresAuth !== false ||
    (route.requiredRoles?.length ?? 0) > 0 ||
    (route.requiredPermissions?.length ?? 0) > 0;

  // Development-only runtime validation for routeable components
  useEffect(() => {
    if (process.env.NODE_ENV === 'development' && route.component) {
      // Get the actual component (unwrap lazy if needed)
      const component = route.component as React.ComponentType<Record<string, unknown>>;

      // Check against blocked list
      if (isBlockedRouteComponent(component)) {
        const name = component.displayName || component.name || 'Unknown';
        console.error(
          `[RouteGuard] BLOCKED: Component "${name}" is in BLOCKED_ROUTE_COMPONENTS ` +
          `and should not be routed directly. It has required props that RouteGuard ` +
          `cannot provide. Use a *RoutePage wrapper instead.\n` +
          `Blocked components: ${BLOCKED_ROUTE_COMPONENTS.join(', ')}\n` +
          `Route path: ${route.path}`
        );
      }

      // Heuristic check for suspicious patterns
      validateRouteableAtRuntime(component, route.path);
    }
  }, [route.component, route.path]);

  // Show timeout error if auth check took too long
  if (authTimeout) {
    return <AuthTimeoutError />;
  }

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
  const content = children ? (
    <>{children}</>
  ) : (
    <ErrorBoundary
      FallbackComponent={ErrorFallback}
      onError={(error) => logUIError(error, { scope: 'page', component: 'RouteGuard', route: route.path, severity: 'error' })}
    >
      <Suspense fallback={<PageSkeleton variant={route.skeletonVariant || 'default'} />}>
        <Component />
      </Suspense>
    </ErrorBoundary>
  );

  if (!requiresAuth) {
    return content;
  }

  return <TenantRequiredGate>{content}</TenantRequiredGate>;
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
