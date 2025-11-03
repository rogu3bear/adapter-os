import React from 'react';
import type { UserRole } from '@/api/types';
import type { LucideIcon } from 'lucide-react';
import { routes, type RouteConfig, canAccessRoute } from '@/config/routes';

export interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

export interface NavGroup {
  title: string;
  items: NavItem[];
  roles?: UserRole[];
}

/**
 * Generate navigation groups from route configuration
 * Filters routes by user role and groups by navGroup
 */
export function generateNavigationGroups(userRole?: UserRole): NavGroup[] {
  // Filter routes that have navigation metadata and are accessible
  const navRoutes = routes.filter(route => 
    route.navGroup && 
    route.navTitle && 
    route.navIcon &&
    canAccessRoute(route, userRole)
  );

  // Group routes by navGroup
  const groupMap = new Map<string, RouteConfig[]>();
  
  for (const route of navRoutes) {
    const group = route.navGroup!;
    if (!groupMap.has(group)) {
      groupMap.set(group, []);
    }
    groupMap.get(group)!.push(route);
  }

  // Convert to NavGroup array and sort
  const navGroups: NavGroup[] = [];
  
  for (const [groupTitle, groupRoutes] of groupMap.entries()) {
    // Sort routes by navOrder
    groupRoutes.sort((a, b) => (a.navOrder ?? 0) - (b.navOrder ?? 0));
    
    // Find if any route in this group has role restrictions
    const restrictedRoles = new Set<UserRole>();
    for (const route of groupRoutes) {
      if (route.requiredRoles) {
        route.requiredRoles.forEach(role => restrictedRoles.add(role));
      }
    }
    
    const navItems: NavItem[] = groupRoutes.map(route => ({
      to: route.path,
      label: route.navTitle!,
      icon: route.navIcon!,
    }));

    navGroups.push({
      title: groupTitle,
      items: navItems,
      roles: restrictedRoles.size > 0 ? Array.from(restrictedRoles) : undefined,
    });
  }

  // Sort groups by title (or could use a predefined order)
  navGroups.sort((a, b) => {
    const order = ['Home', 'ML Pipeline', 'Monitoring', 'Operations', 'Communication', 'Compliance', 'Administration'];
    const aIndex = order.indexOf(a.title);
    const bIndex = order.indexOf(b.title);
    if (aIndex >= 0 && bIndex >= 0) return aIndex - bIndex;
    if (aIndex >= 0) return -1;
    if (bIndex >= 0) return 1;
    return a.title.localeCompare(b.title);
  });

  return navGroups;
}

/**
 * Check if a navigation group should be visible based on user role
 */
export function shouldShowNavGroup(group: NavGroup, userRole?: UserRole): boolean {
  if (!group.roles || group.roles.length === 0) return true;
  return userRole ? group.roles.includes(userRole) : false;
}
