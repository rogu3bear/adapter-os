import React from 'react';
import { Link } from 'react-router-dom';
import { useBreadcrumbs } from '@/hooks/useBreadcrumbs';
import {
  Breadcrumb,
  BreadcrumbList,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from './ui/breadcrumb';
import { ChevronRight, Home } from 'lucide-react';

/**
 * BreadcrumbNavigation - Stateless breadcrumb component
 * Derives breadcrumbs from current URL pathname using route configuration
 */
export function BreadcrumbNavigation() {
  const breadcrumbs = useBreadcrumbs();

  // Filter out Home since we'll add it explicitly, and non-clickable items
  const displayBreadcrumbs = breadcrumbs.filter(b => b.id !== 'home' && b.href !== '#');

  if (displayBreadcrumbs.length === 0) {
    return null;
  }

  return (
    <Breadcrumb className="mb-4">
      <BreadcrumbList>
        <BreadcrumbItem>
          <BreadcrumbLink href="/" className="flex items-center gap-1">
            <Home className="h-4 w-4" />
            <span className="sr-only">Home</span>
          </BreadcrumbLink>
        </BreadcrumbItem>
        
        {displayBreadcrumbs.map((item, index) => {
          const isLast = index === displayBreadcrumbs.length - 1;
          const Icon = item.icon;
          
          return (
            <React.Fragment key={item.id}>
              <BreadcrumbSeparator>
                <ChevronRight className="h-4 w-4" />
              </BreadcrumbSeparator>
              <BreadcrumbItem>
                {isLast ? (
                  <BreadcrumbPage className="flex items-center gap-1">
                    {Icon && <Icon className="h-4 w-4" />}
                    {item.label}
                  </BreadcrumbPage>
                ) : (
                  <BreadcrumbLink asChild>
                    <Link to={item.href} className="flex items-center gap-1">
                      {Icon && <Icon className="h-4 w-4" />}
                      {item.label}
                    </Link>
                  </BreadcrumbLink>
                )}
              </BreadcrumbItem>
            </React.Fragment>
          );
        })}
      </BreadcrumbList>
    </Breadcrumb>
  );
}
