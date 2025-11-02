import React from 'react';
import { Navigate } from 'react-router-dom';
import { useAuth } from '@/providers/CoreProviders';
import type { RouteConfig } from '@/config/routes';
import { canAccessRoute } from '@/config/routes';

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
  const requiresAuth = route.requiresAuth !== false || (route.requiredRoles?.length ?? 0) > 0;

  // Show loading state while auth is being verified
  if (requiresAuth && isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
      </div>
    );
  }

  // If route requires authentication and user is not authenticated
  if (requiresAuth && !user) {
    return <Navigate to="/login" replace />;
  }

  // Check if user has required role
  if (route.requiredRoles && route.requiredRoles.length > 0 && user && !canAccessRoute(route, user.role)) {
    return <Navigate to={fallbackPath} replace />;
  }

  const Component = route.component;
  if (children) {
    return <>{children}</>;
  }

  return <Component />;
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
