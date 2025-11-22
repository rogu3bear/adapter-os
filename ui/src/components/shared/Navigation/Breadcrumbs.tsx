import React from 'react';
import { Link, useLocation } from 'react-router-dom';
import { ChevronRight, Home } from 'lucide-react';

import { cn } from '../../ui/utils';
import {
  Breadcrumb,
  BreadcrumbList,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbPage,
  BreadcrumbSeparator,
  BreadcrumbEllipsis,
} from '../../ui/breadcrumb';

export interface BreadcrumbItemConfig {
  /** Unique identifier for the breadcrumb item */
  id: string;
  /** Display label for the breadcrumb */
  label: string;
  /** Navigation path (optional for current page) */
  href?: string;
  /** Optional icon component */
  icon?: React.ComponentType<{ className?: string }>;
}

export interface BreadcrumbsProps {
  /** Array of breadcrumb items to display */
  items: BreadcrumbItemConfig[];
  /** Show home icon as first item (default: true) */
  showHome?: boolean;
  /** Home path (default: "/") */
  homePath?: string;
  /** Maximum items to show before collapsing (default: 4) */
  maxItems?: number;
  /** Additional CSS classes */
  className?: string;
  /** Custom separator element */
  separator?: React.ReactNode;
}

/**
 * Breadcrumbs - Navigation breadcrumb trail component
 *
 * Displays hierarchical navigation path with support for icons,
 * collapsible items, and React Router integration.
 *
 * @example
 * ```tsx
 * <Breadcrumbs
 *   items={[
 *     { id: 'adapters', label: 'Adapters', href: '/adapters' },
 *     { id: 'detail', label: 'my-adapter', href: '/adapters/my-adapter' },
 *     { id: 'settings', label: 'Settings' }
 *   ]}
 * />
 * ```
 */
export function Breadcrumbs({
  items,
  showHome = true,
  homePath = '/',
  maxItems = 4,
  className,
  separator,
}: BreadcrumbsProps) {
  const location = useLocation();

  if (items.length === 0 && !showHome) {
    return null;
  }

  // Collapse middle items if we have more than maxItems
  const shouldCollapse = items.length > maxItems;
  const visibleItems = shouldCollapse
    ? [items[0], ...items.slice(-2)]
    : items;
  const hiddenCount = shouldCollapse ? items.length - 3 : 0;

  const renderSeparator = () => (
    <BreadcrumbSeparator>
      {separator ?? <ChevronRight className="h-4 w-4" />}
    </BreadcrumbSeparator>
  );

  const renderItem = (item: BreadcrumbItemConfig, isLast: boolean) => {
    const Icon = item.icon;
    const isActive = item.href === location.pathname;

    if (isLast || !item.href) {
      return (
        <BreadcrumbPage className="flex items-center gap-1.5">
          {Icon && <Icon className="h-4 w-4" />}
          <span>{item.label}</span>
        </BreadcrumbPage>
      );
    }

    return (
      <BreadcrumbLink asChild>
        <Link
          to={item.href}
          className={cn(
            "flex items-center gap-1.5",
            isActive && "text-foreground font-medium"
          )}
        >
          {Icon && <Icon className="h-4 w-4" />}
          <span>{item.label}</span>
        </Link>
      </BreadcrumbLink>
    );
  };

  return (
    <Breadcrumb className={className}>
      <BreadcrumbList>
        {/* Home link */}
        {showHome && (
          <>
            <BreadcrumbItem>
              <BreadcrumbLink asChild>
                <Link to={homePath} className="flex items-center gap-1">
                  <Home className="h-4 w-4" />
                  <span className="sr-only">Home</span>
                </Link>
              </BreadcrumbLink>
            </BreadcrumbItem>
            {items.length > 0 && renderSeparator()}
          </>
        )}

        {/* First visible item (if collapsing) */}
        {shouldCollapse && visibleItems[0] && (
          <>
            <BreadcrumbItem>
              {renderItem(visibleItems[0], false)}
            </BreadcrumbItem>
            {renderSeparator()}
          </>
        )}

        {/* Ellipsis for collapsed items */}
        {shouldCollapse && (
          <>
            <BreadcrumbItem>
              <BreadcrumbEllipsis />
              <span className="sr-only">{hiddenCount} more items</span>
            </BreadcrumbItem>
            {renderSeparator()}
          </>
        )}

        {/* Remaining visible items */}
        {(shouldCollapse ? visibleItems.slice(1) : visibleItems).map((item, index, arr) => {
          const isLast = index === arr.length - 1;
          return (
            <React.Fragment key={item.id}>
              <BreadcrumbItem>
                {renderItem(item, isLast)}
              </BreadcrumbItem>
              {!isLast && renderSeparator()}
            </React.Fragment>
          );
        })}
      </BreadcrumbList>
    </Breadcrumb>
  );
}

/**
 * Hook to generate breadcrumb items from current route
 *
 * @param routeLabels - Map of route paths to display labels
 * @returns Array of BreadcrumbItemConfig
 *
 * @example
 * ```tsx
 * const breadcrumbs = useBreadcrumbsFromRoute({
 *   '/adapters': 'Adapters',
 *   '/adapters/:id': 'Adapter Details',
 * });
 * ```
 */
export function useBreadcrumbsFromRoute(
  routeLabels: Record<string, string>
): BreadcrumbItemConfig[] {
  const location = useLocation();
  const pathSegments = location.pathname.split('/').filter(Boolean);

  const breadcrumbs: BreadcrumbItemConfig[] = [];
  let currentPath = '';

  for (const segment of pathSegments) {
    currentPath += `/${segment}`;
    const label = routeLabels[currentPath] ?? segment;

    breadcrumbs.push({
      id: currentPath,
      label,
      href: currentPath,
    });
  }

  return breadcrumbs;
}
