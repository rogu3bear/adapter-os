import { useMemo } from 'react';
import { useLocation } from 'react-router-dom';
import { buildBreadcrumbChain, type BreadcrumbItem } from '@/utils/breadcrumbs';

/**
 * Stateless hook that derives breadcrumbs from current URL pathname
 * Computed on-demand from route configuration - no persistence needed
 */
export function useBreadcrumbs(): BreadcrumbItem[] {
  const location = useLocation();

  const breadcrumbs = useMemo(() => {
    return buildBreadcrumbChain(location.pathname);
  }, [location.pathname]);

  return breadcrumbs;
}

