import { routes, canAccessRoute, type RouteConfig } from '@/config/routes';
import type { LucideIcon } from 'lucide-react';

export interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  disabled?: boolean;
  external?: boolean;
}

export interface NavGroup {
  title: string;
  items: NavItem[];
  roles?: string[];
  order?: number;
}

/**
 * Generate navigation groups from centralized route configuration
 * Filters routes by user role and organizes them into logical groups
 */
export function generateNavigationGroups(userRole?: string): NavGroup[] {
  const groupsMap = new Map<string, NavGroup>();

  // Process each route from the central config
  for (const route of routes) {
    // Skip routes without navigation metadata
    if (!route.navGroup || !route.navTitle) {
      continue;
    }

    // Check if user can access this route
    if (!canAccessRoute(route, userRole)) {
      continue;
    }

    const groupKey = route.navGroup;
    const group = groupsMap.get(groupKey) || {
      title: groupKey,
      items: [],
      roles: route.requiredRoles,
      order: 0,
    };

    // Add the route to the group
    group.items.push({
      to: route.path,
      label: route.navTitle,
      icon: route.navIcon,
      disabled: route.disabled,
      external: route.external,
    });

    groupsMap.set(groupKey, group);
  }

  // Convert map to array and sort
  const groups = Array.from(groupsMap.values());

  // Sort groups by predefined order (Home first, then alphabetical)
  const groupOrder = ['Home', 'ML Pipeline', 'Monitoring', 'Operations', 'Communication', 'Compliance', 'Administration'];
  groups.sort((a, b) => {
    const aIndex = groupOrder.indexOf(a.title);
    const bIndex = groupOrder.indexOf(b.title);

    // Known groups get priority based on order array
    if (aIndex !== -1 && bIndex !== -1) {
      return aIndex - bIndex;
    }
    if (aIndex !== -1) return -1;
    if (bIndex !== -1) return 1;

    // Alphabetical fallback
    return a.title.localeCompare(b.title);
  });

  // Sort items within each group by navOrder
  for (const group of groups) {
    group.items.sort((a, b) => {
      const aRoute = routes.find(r => r.path === a.to);
      const bRoute = routes.find(r => r.path === b.to);
      const aOrder = aRoute?.navOrder ?? 999;
      const bOrder = bRoute?.navOrder ?? 999;
      return aOrder - bOrder;
    });
  }

  return groups;
}

/**
 * Check if a navigation group should be shown to a user
 * Handles role-based access control for entire navigation groups
 */
export function shouldShowNavGroup(group: NavGroup, userRole?: string): boolean {
  // If group has no role restrictions, show to everyone
  if (!group.roles || group.roles.length === 0) {
    return true;
  }

  // If user has no role, hide restricted groups
  if (!userRole) {
    return false;
  }

  // Check if user's role is in the allowed roles
  return group.roles.includes(userRole);
}

/**
 * Get all accessible routes for a user role
 * Useful for command palette and search functionality
 */
export function getAccessibleRoutes(userRole?: string): RouteConfig[] {
  return routes.filter(route => canAccessRoute(route, userRole));
}

/**
 * Find a route by path
 */
export function findRouteByPath(path: string): RouteConfig | undefined {
  return routes.find(route => route.path === path);
}

/**
 * Get navigation breadcrumbs for a path
 */
export function getBreadcrumbs(path: string): Array<{ label: string; to?: string }> {
  const route = findRouteByPath(path);
  if (!route) return [];

  const breadcrumbs = [];

  // Add group breadcrumb if available
  if (route.navGroup) {
    breadcrumbs.push({ label: route.navGroup });
  }

  // Add page breadcrumb
  breadcrumbs.push({ label: route.navTitle, to: route.path });

  return breadcrumbs;
}
