import React from 'react';
import { Button } from './button';
import { HelpTooltip } from './help-tooltip';

interface ActionItem {
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  color?: string;
  helpId?: string;
  disabled?: boolean;
  disabledTitle?: string;
  onClick: () => void;
}

interface ActionGridProps {
  actions: ActionItem[];
  columns?: 1 | 2 | 3 | 4;
}

export function ActionGrid({ actions, columns = 4 }: ActionGridProps) {
  const gridCols = {
    1: 'grid-cols-1',
    2: 'grid-cols-1 sm:grid-cols-2',
    3: 'grid-cols-1 sm:grid-cols-2 md:grid-cols-3',
    4: 'grid-cols-1 sm:grid-cols-2 md:grid-cols-4'
  };

  return (
    <div className={`grid ${gridCols[columns]} gap-3`} aria-label="Quick actions" role="list">
      {actions.map((action, index) => {
        const Icon = action.icon;
        const button = (
          <Button
            variant="outline"
            className="justify-start h-auto py-4 w-full"
            aria-label={`Quick action: ${action.label}`}
            disabled={action.disabled}
            title={action.disabled ? action.disabledTitle : undefined}
            onClick={action.onClick}
          >
            <div className="flex items-center gap-3">
              <Icon className={`h-5 w-5 ${action.color || ''}`} aria-hidden="true" />
              <span className="font-medium">{action.label}</span>
            </div>
          </Button>
        );

        if (action.helpId) {
          return (
            <HelpTooltip key={`${action.label}-${index}`} helpId={action.helpId}>
              {button}
            </HelpTooltip>
          );
        }

        return <React.Fragment key={`${action.label}-${index}`}>{button}</React.Fragment>;
      })}
    </div>
  );
}
