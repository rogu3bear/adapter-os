import React from 'react';
import { Button } from '@/components/ui/button';
import { LucideIcon } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface DashboardPageHeaderProps {
  title: string;
  description?: string;
  viewControls?: React.ReactNode;
  refreshAction?: {
    label?: string;
    icon?: LucideIcon;
    onClick: () => void;
    loading?: boolean;
  };
  className?: string;
}

export function DashboardPageHeader({
  title,
  description,
  viewControls,
  refreshAction,
  className
}: DashboardPageHeaderProps) {
  const RefreshIcon = refreshAction?.icon;

  return (
    <div className={cn("flex items-center justify-between", className)}>
      <div>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        {description && (
          <p className="text-muted-foreground mt-1">{description}</p>
        )}
      </div>
      <div className="flex items-center gap-3">
        {viewControls}
        {refreshAction && (
          <Button
            onClick={refreshAction.onClick}
            variant="outline"
            disabled={refreshAction.loading}
          >
            {RefreshIcon && <RefreshIcon className={cn("mr-2 h-4 w-4", refreshAction.loading && "animate-spin")} />}
            {refreshAction.label || 'Refresh'}
          </Button>
        )}
      </div>
    </div>
  );
}

