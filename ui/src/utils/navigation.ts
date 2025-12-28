import { routes, canAccessRoute, type RouteConfig } from '@/config/routes';
import { PRIMARY_SPINE } from '@/config/routes_manifest';
import type { LucideIcon } from 'lucide-react';
import type { UserRole } from '@/api/types';
import { UiMode } from '@/config/ui-mode';

const DEMO_PRIMARY_SPINE = ['/dashboard', '/workspaces', '/base-models', '/documents', '/training', '/chat'] as const;

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
export function generateNavigationGroups(
  userRole?: string,
  userPermissions?: string[],
  uiMode: UiMode = UiMode.User,
  options: { demoMode?: boolean } = {},
): NavGroup[] {
  const demoMode = options.demoMode === true;
  const effectiveMode = uiMode === UiMode.Kernel ? UiMode.Builder : uiMode;
  const spine = demoMode ? DEMO_PRIMARY_SPINE : PRIMARY_SPINE;
  const spineOrder = new Map<string, number>(spine.map((path, index) => [path, index]));
  const groupsMap = new Map<string, NavGroup>();

  const isDeveloper = userRole?.toLowerCase() === 'developer';

  // Process each route from the central config
  for (const route of routes) {
    // Skip routes without navigation metadata
    if (!route.navTitle) {
      continue;
    }

    // Keep sidebar focused on primary spine pages
    if (!spineOrder.has(route.path)) {
      continue;
    }

    // Developer bypasses UI mode filtering - sees all routes.
    // Demo mode also bypasses UI mode filtering (demo nav spans chat + training).
    if (!isDeveloper && !demoMode && route.modes && !route.modes.includes(effectiveMode)) {
      continue;
    }

    // Check if user can access this route
    if (!canAccessRoute(route, userRole as UserRole | undefined, userPermissions)) {
      continue;
    }

    const groupKey = route.navGroup ?? route.cluster ?? 'Other';
    const routeOrder = spineOrder.get(route.path) ?? Number.MAX_SAFE_INTEGER;
    const group = groupsMap.get(groupKey) || {
      title: groupKey,
      items: [],
      roles: route.requiredRoles,
      order: routeOrder,
    };

    // Keep the earliest spine position for deterministic grouping order
    group.order = Math.min(group.order ?? Number.MAX_SAFE_INTEGER, routeOrder);

    // Add the route to the group (skip if no icon)
    if (!route.navIcon) {
      continue;
    }

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

  groups.sort((a, b) => {
    const aOrder = a.order ?? Number.MAX_SAFE_INTEGER;
    const bOrder = b.order ?? Number.MAX_SAFE_INTEGER;
    if (aOrder !== bOrder) {
      return aOrder - bOrder;
    }
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
  // Developer role sees all navigation groups
  if (userRole?.toLowerCase() === 'developer') {
    return true;
  }

  // If group has no role restrictions, show to everyone
  if (!group.roles || group.roles.length === 0) {
    return true;
  }

  // If user has no role, hide restricted groups
  if (!userRole) {
    return false;
  }

  // Check if user's role is in the allowed roles (case-insensitive)
  const normalizedRole = userRole.toLowerCase();
  return group.roles.some(role => role.toLowerCase() === normalizedRole);
}

/**
 * Get all accessible routes for a user role
 * Useful for command palette and search functionality
 */
export function getAccessibleRoutes(userRole?: string, userPermissions?: string[]): RouteConfig[] {
  return routes.filter(route => canAccessRoute(route, userRole as UserRole | undefined, userPermissions));
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

  // Prefix with cluster to satisfy IA breadcrumb rule
  if (route.cluster) {
    breadcrumbs.push({ label: route.cluster });
  }

  // Add page breadcrumb
  if (route.navTitle) {
    breadcrumbs.push({ label: route.navTitle, to: route.path });
  }

  return breadcrumbs;
}
