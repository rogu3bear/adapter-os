import React from 'react';
import { Button } from '../button';
import { LucideIcon } from 'lucide-react';
import { cn } from '../utils';

export interface CrudPageHeaderProps {
  title: string;
  description?: string;
  primaryAction?: {
    label: string;
    icon?: LucideIcon;
    onClick: () => void;
    variant?: 'default' | 'outline' | 'secondary' | 'destructive';
  };
  secondaryActions?: React.ReactNode;
  className?: string;
}

export function CrudPageHeader({
  title,
  description,
  primaryAction,
  secondaryActions,
  className
}: CrudPageHeaderProps) {
  const Icon = primaryAction?.icon;

  return (
    <div className={cn("flex items-center justify-between", className)}>
      <div>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        {description && (
          <p className="text-muted-foreground mt-1">{description}</p>
        )}
      </div>
      <div className="flex items-center gap-2">
        {secondaryActions}
        {primaryAction && (
          <Button
            onClick={primaryAction.onClick}
            variant={primaryAction.variant || 'default'}
          >
            {Icon && <Icon className="mr-2 h-4 w-4" />}
            {primaryAction.label}
          </Button>
        )}
      </div>
    </div>
  );
}

