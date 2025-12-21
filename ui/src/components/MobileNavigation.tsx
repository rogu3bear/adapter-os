import React, { useState } from 'react';
import { Button } from './ui/button';
import { ChevronDown, ChevronRight } from 'lucide-react';
import type { NavGroup } from '@/utils/navigation';

interface MobileNavigationProps {
  groups: NavGroup[];
  onNavigate: (path: string) => void;
  userRole?: string;
}

/**
 * MobileNavigation - Preserves hierarchical navigation structure on mobile
 * Uses collapsible groups with touch-optimized UI (≥44px hit targets per WCAG 2.1)
 */
export function MobileNavigation({ groups, onNavigate, userRole }: MobileNavigationProps) {
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});

  const toggleGroup = (groupTitle: string) => {
    setCollapsedGroups(prev => ({
      ...prev,
      [groupTitle]: !prev[groupTitle]
    }));
  };

  return (
    <div className="md:hidden space-y-1">
      {groups.map((group) => {
        const isCollapsed = collapsedGroups[group.title];
        
        return (
          <div key={group.title} className="mb-4">
            {/* Group header with touch-optimized hit target (min 44px height) */}
            <button
              onClick={() => toggleGroup(group.title)}
              className="flex items-center justify-between w-full px-3 py-3 text-xs font-semibold text-muted-foreground uppercase tracking-wider hover:text-foreground transition-colors min-h-[44px]"
              aria-expanded={!isCollapsed}
              aria-label={`Toggle ${group.title} menu`}
            >
              <span>{group.title}</span>
              {isCollapsed ? (
                <ChevronRight className="h-4 w-4 flex-shrink-0" />
              ) : (
                <ChevronDown className="h-4 w-4 flex-shrink-0" />
              )}
            </button>
            
            {/* Collapsible items */}
            {!isCollapsed && (
              <div className="mt-1 space-y-1 pl-1">
                {group.items.map((item) => {
                  const Icon = item.icon;
                  return (
                    <Button
                      key={item.to}
                      variant="ghost"
                      className="w-full justify-start h-12 px-4 min-h-[44px]" // WCAG 2.1 minimum touch target
                      onClick={() => onNavigate(item.to)}
                      aria-label={`Navigate to ${item.label}`}
                    >
                      <Icon className="h-5 w-5 mr-3 flex-shrink-0" />
                      <span className="text-sm font-medium">{item.label}</span>
                    </Button>
                  );
                })}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

