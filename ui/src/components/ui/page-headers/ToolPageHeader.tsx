import React from 'react';
import { Button } from '@/components/ui/button';
import { LucideIcon } from 'lucide-react';
import { cn } from '@/components/ui/utils';

export interface ToolPageHeaderProps {
  title: string;
  description?: string;
  primaryAction?: {
    label: string;
    icon?: LucideIcon;
    onClick: () => void;
    variant?: 'default' | 'outline' | 'secondary' | 'destructive';
    loading?: boolean;
    size?: 'default' | 'sm' | 'lg' | 'icon';
  };
  secondaryActions?: React.ReactNode;
  className?: string;
}

export function ToolPageHeader({
  title,
  description,
  primaryAction,
  secondaryActions,
  className
}: ToolPageHeaderProps) {
  const Icon = primaryAction?.icon;

  return (
    <div className={cn("flex items-center justify-between", className)}>
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
        {description && (
          <p className="text-sm text-muted-foreground mt-1">{description}</p>
        )}
      </div>
      <div className="flex items-center gap-2">
        {secondaryActions}
        {primaryAction && (
          <Button
            onClick={primaryAction.onClick}
            variant={primaryAction.variant || 'default'}
            size={primaryAction.size || 'lg'}
            disabled={primaryAction.loading}
            className="min-w-[120px]"
          >
            {Icon && <Icon className={cn("mr-2 h-4 w-4", primaryAction.loading && "animate-spin")} />}
            {primaryAction.label}
          </Button>
        )}
      </div>
    </div>
  );
}

