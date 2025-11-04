import { useMemo } from 'react';
import { useLocation } from 'react-router-dom';
import { getBreadcrumbs } from '@/utils/navigation';

export interface BreadcrumbItem {
  label: string;
  to?: string;
}

/**
 * Hook to generate breadcrumbs based on current route
 * Automatically updates when location changes
 */
export function useBreadcrumbs(): BreadcrumbItem[] {
  const location = useLocation();

  return useMemo(() => {
    return getBreadcrumbs(location.pathname);
  }, [location.pathname]);
}
