import { useLocation, Link } from 'react-router-dom';
import { getBreadcrumbs } from '@/config/routes';
import { cn } from '@/components/ui/utils';

interface HeaderBreadcrumbsProps {
  className?: string;
}

export function HeaderBreadcrumbs({ className }: HeaderBreadcrumbsProps) {
  const location = useLocation();
  const breadcrumbs = getBreadcrumbs(location.pathname);

  if (breadcrumbs.length === 0) {
    return null;
  }

  return (
    <nav aria-label="Breadcrumb" className={cn('hidden md:flex items-center gap-1 text-sm truncate', className)}>
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
    </nav>
  );
}
