import { useLocation, useParams, Link } from 'react-router-dom';
import { getBreadcrumbs, getClusterForPath } from '@/config/routes';
import { cn } from '@/components/ui/utils';

interface HeaderBreadcrumbsProps {
  className?: string;
}

export function HeaderBreadcrumbs({ className }: HeaderBreadcrumbsProps) {
  const location = useLocation();
  const params = useParams();
  const clusterLabel = getClusterForPath(location.pathname);
  const breadcrumbs = getBreadcrumbs(location.pathname, params as Record<string, string>);

  if (!clusterLabel && breadcrumbs.length === 0) {
    return null;
  }

  return (
    <nav aria-label="Breadcrumb" className={cn('hidden md:flex items-center gap-1 text-sm truncate', className)}>
      {clusterLabel ? (
        <span className="flex items-center gap-1 text-foreground font-semibold">
          <span>{clusterLabel}</span>
          {breadcrumbs.length > 0 && <span className="text-muted-foreground/30">/</span>}
        </span>
      ) : null}

      {breadcrumbs.map((crumb, index) => {
        const isLast = index === breadcrumbs.length - 1;
        return (
          <span key={`${crumb.path}-${crumb.label}`} className="flex items-center gap-1">
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
    </nav>
  );
}
