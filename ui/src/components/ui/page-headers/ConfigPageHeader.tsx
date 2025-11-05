import React from 'react';
import { Button } from '../button';
import { LucideIcon } from 'lucide-react';
import { cn } from '../utils';

export interface ConfigPageHeaderProps {
  title: string;
  description?: string;
  primaryAction?: {
    label: string;
    icon?: LucideIcon;
    onClick: () => void;
    variant?: 'default' | 'outline' | 'secondary' | 'destructive';
    loading?: boolean;
  };
  secondaryActions?: React.ReactNode;
  className?: string;
}

export function ConfigPageHeader({
  title,
  description,
  primaryAction,
  secondaryActions,
  className
}: ConfigPageHeaderProps) {
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
            disabled={primaryAction.loading}
          >
            {Icon && <Icon className={cn("mr-2 h-4 w-4", primaryAction.loading && "animate-spin")} />}
            {primaryAction.label}
          </Button>
        )}
      </div>
    </div>
  );
}

