//! Bulk Action Bar Component
//!
//! Provides consistent bulk action patterns across the application.
//!
//! Citations:
//! - docs/Smashing Design Techniques.md L300-L350 - Bulk operation UX patterns
//! - ui/src/components/Adapters.tsx L1-L50 - Table implementation patterns

import React from 'react';
import { Button } from './button';
import { X } from 'lucide-react';

export interface BulkAction {
  id: string;
  label: string;
  variant?: 'default' | 'destructive' | 'outline' | 'secondary' | 'ghost' | 'link';
  handler: (selectedItems: string[]) => void | Promise<void>;
  disabled?: boolean;
}

interface BulkActionBarProps {
  selectedItems: string[];
  actions: BulkAction[];
  onClearSelection: () => void;
  itemName?: string;
  className?: string;
}

export function BulkActionBar({
  selectedItems,
  actions,
  onClearSelection,
  itemName = 'items',
  className = ''
}: BulkActionBarProps) {
  if (selectedItems.length === 0) return null;

  return (
    <div className={`fixed bottom-4 left-1/2 transform -translate-x-1/2
                     bg-primary text-primary-foreground px-4 py-3 rounded-lg
                     shadow-lg border border-border z-50 flex items-center gap-4
                     min-w-[250px] sm:min-w-[300px] ${className}`}>
      <div className="flex items-center gap-2 flex-1">
        <span className="text-sm font-medium">
          {selectedItems.length} {itemName} selected
        </span>
        <Button
          variant="ghost"
          size="sm"
          onClick={onClearSelection}
          className="h-6 w-6 p-0 hover:bg-primary-foreground/10"
          aria-label="Clear selection"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      <div className="flex items-center gap-2">
        {actions.map((action) => (
          <Button
            key={action.id}
            variant={action.variant || 'secondary'}
            size="sm"
            onClick={() => action.handler(selectedItems)}
            className="text-xs"
            disabled={action.disabled}
          >
            {action.label}
          </Button>
        ))}
      </div>
    </div>
  );
}

export default BulkActionBar;
