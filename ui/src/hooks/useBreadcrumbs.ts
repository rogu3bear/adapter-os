import { useMemo } from 'react';
import { useLocation } from 'react-router-dom';
import { buildBreadcrumbChain, type BreadcrumbItem } from '@/utils/breadcrumbs';

/**
 * Hook to generate breadcrumbs based on current route
 * Automatically updates when location changes
 */
export function useBreadcrumbs(): BreadcrumbItem[] {
  const location = useLocation();

  return useMemo(() => {
    return buildBreadcrumbChain(location.pathname);
  }, [location.pathname]);
}

// Re-export the type for convenience
export type { BreadcrumbItem };
