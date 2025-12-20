import { Link, useLocation } from 'react-router-dom';
import type { RouteCluster } from '@/config/routes';
import { getBreadcrumbs, getClusterForPath } from '@/config/routes';
import { cn } from '@/lib/utils';

interface ClusterBreadcrumbProps {
  cluster?: RouteCluster;
  className?: string;
}

/**
 * Renders a breadcrumb trail prefixed with the current cluster.
 * Uses route metadata when an explicit cluster is not provided.
 */
export function ClusterBreadcrumb({ cluster: clusterOverride, className }: ClusterBreadcrumbProps) {
  const location = useLocation();
  const cluster = clusterOverride ?? getClusterForPath(location.pathname);
  const breadcrumbs = getBreadcrumbs(location.pathname);

  if (!cluster && breadcrumbs.length === 0) {
    return null;
  }

  return (
    <nav
      aria-label="Breadcrumb"
      className={cn('flex items-center gap-2 text-sm text-muted-foreground', className)}
    >
      {cluster ? <span className="font-semibold text-foreground">{cluster}</span> : null}
      {breadcrumbs.length > 0 ? (
        <div className="flex items-center gap-1 truncate">
          {breadcrumbs.map((crumb, index) => {
            const isLast = index === breadcrumbs.length - 1;
            return (
              <span key={crumb.path} className="flex items-center gap-1">
                {index > 0 && <span className="text-muted-foreground/30">/</span>}
                {isLast ? (
                  <span className="text-foreground truncate">{crumb.label}</span>
                ) : (
                  <Link
                    to={crumb.path}
                    className="text-muted-foreground hover:text-foreground transition-colors truncate"
                  >
                    {crumb.label}
                  </Link>
                )}
              </span>
            );
          })}
        </div>
      ) : null}
    </nav>
  );
}

