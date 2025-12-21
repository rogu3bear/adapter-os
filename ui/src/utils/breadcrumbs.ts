import React from 'react';
import type { RouteConfig } from '@/config/routes';
import { routes, getRouteByPath } from '@/config/routes';

export interface BreadcrumbItem {
  id: string;
  label: string;
  href: string;
  icon?: React.ComponentType<{ className?: string }>;
}

/**
 * Build breadcrumb chain from URL pathname
 * Derives breadcrumbs deterministically from route config
 */
export function buildBreadcrumbChain(pathname: string): BreadcrumbItem[] {
  const breadcrumbs: BreadcrumbItem[] = [];

  // Always include Home
  breadcrumbs.push({
    id: 'home',
    label: 'Home',
    href: '/',
  });

  // If pathname is root or dashboard, we're done
  if (pathname === '/' || pathname === '/dashboard') {
    return breadcrumbs;
  }

  // Find matching route
  const route = getRouteByPath(pathname);
  
  if (route) {
    // If route has navigation metadata, use it
    if (route.navGroup && route.navTitle) {
      // Add group as intermediate breadcrumb if it exists
      if (route.navGroup !== 'Home') {
        breadcrumbs.push({
          id: `group-${route.navGroup.toLowerCase().replace(/\s+/g, '-')}`,
          label: route.navGroup,
          href: '#', // Groups don't have direct links
        });
      }
      
      // Add current page
      breadcrumbs.push({
        id: `page-${route.path}`,
        label: route.navTitle,
        href: route.path,
        icon: route.navIcon,
      });
    } else {
      // Fallback: use path segments
      const segments = pathname.split('/').filter(Boolean);
      segments.forEach((segment, index) => {
        const href = '/' + segments.slice(0, index + 1).join('/');
        const route = getRouteByPath(href);
        
        breadcrumbs.push({
          id: `segment-${index}`,
          label: route?.navTitle || segment.charAt(0).toUpperCase() + segment.slice(1).replace(/-/g, ' '),
          href,
          icon: route?.navIcon,
        });
      });
    }
  } else {
    // No route found, create breadcrumbs from path segments
    const segments = pathname.split('/').filter(Boolean);
    segments.forEach((segment, index) => {
      const href = '/' + segments.slice(0, index + 1).join('/');
      
      breadcrumbs.push({
        id: `segment-${index}`,
        label: segment.charAt(0).toUpperCase() + segment.slice(1).replace(/-/g, ' '),
        href,
      });
    });
  }

  return breadcrumbs;
}

