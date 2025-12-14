/**
 * LeftRailTabs - Tab switcher for the left rail
 *
 * Displays three tabs: Sessions, Datasets, Stacks
 * with underline indicator for active tab.
 */

import { MessageSquare, Database, Layers } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { LeftRailTab } from '@/contexts/WorkbenchContext';

interface LeftRailTabsProps {
  activeTab: LeftRailTab;
  onTabChange: (tab: LeftRailTab) => void;
}

const TABS: Array<{ id: LeftRailTab; label: string; icon: typeof MessageSquare }> = [
  { id: 'sessions', label: 'Sessions', icon: MessageSquare },
  { id: 'datasets', label: 'Datasets', icon: Database },
  { id: 'stacks', label: 'Stacks', icon: Layers },
];

export function LeftRailTabs({ activeTab, onTabChange }: LeftRailTabsProps) {
  return (
    <div
      className="flex border-b bg-background"
      role="tablist"
      aria-label="Workbench navigation"
      data-testid="left-rail-tabs"
    >
      {TABS.map(({ id, label, icon: Icon }) => {
        const isActive = activeTab === id;
        return (
          <button
            key={id}
            role="tab"
            aria-selected={isActive}
            aria-controls={`${id}-panel`}
            tabIndex={isActive ? 0 : -1}
            onClick={() => onTabChange(id)}
            onKeyDown={(e) => {
              const currentIndex = TABS.findIndex((t) => t.id === activeTab);
              if (e.key === 'ArrowRight') {
                const nextIndex = (currentIndex + 1) % TABS.length;
                onTabChange(TABS[nextIndex].id);
              } else if (e.key === 'ArrowLeft') {
                const prevIndex = (currentIndex - 1 + TABS.length) % TABS.length;
                onTabChange(TABS[prevIndex].id);
              }
            }}
            className={cn(
              'flex-1 flex items-center justify-center gap-1.5 px-3 py-2.5 text-sm font-medium transition-colors',
              'border-b-2 -mb-px',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
              isActive
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground hover:border-muted-foreground/30'
            )}
            data-testid={`tab-${id}`}
          >
            <Icon className="h-4 w-4" aria-hidden />
            <span>{label}</span>
          </button>
        );
      })}
    </div>
  );
}
