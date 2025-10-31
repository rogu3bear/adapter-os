// 【ui/src/layout/RootLayout.tsx§196-250】 - Mobile sidebar pattern
// 【ui/src/layout/RootLayout.tsx§78-88】 - NavGroup interface
// Simplified mobile navigation - top-level categories only
import React from 'react';
import { Button } from './ui/button';
import type { UserRole } from '@/api/types';

interface NavItem {
  to: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}

interface NavGroup {
  title: string;
  items: NavItem[];
  roles?: UserRole[];
}

interface MobileNavigationProps {
  groups: NavGroup[];
  onNavigate: (path: string) => void;
  userRole?: UserRole;
}

export function MobileNavigation({ groups, onNavigate, userRole }: MobileNavigationProps) {
  // Flatten navigation to top-level items only on mobile
  // Filter by role if specified
  const shouldShowGroup = (group: NavGroup): boolean => {
    if (!group.roles || group.roles.length === 0) return true;
    return userRole ? group.roles.includes(userRole) : false;
  };

  const mobileItems = groups
    .filter(shouldShowGroup)
    .flatMap(group => 
      group.items.map(item => ({ ...item, group: group.title }))
    );
  
  return (
    <div className="md:hidden space-y-1">
      {/* Simplified list without collapsible groups */}
      {mobileItems.map(item => {
        const Icon = item.icon;
        return (
          <Button
            key={item.to}
            variant="ghost"
            className="w-full justify-start h-12 px-4" // Minimum 44px touch target (WCAG 2.1)
            onClick={() => onNavigate(item.to)}
            aria-label={`Navigate to ${item.label}`}
          >
            <Icon className="h-5 w-5 mr-3 flex-shrink-0" />
            <span className="text-sm font-medium">{item.label}</span>
          </Button>
        );
      })}
    </div>
  );
}

