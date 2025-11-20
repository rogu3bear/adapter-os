import React from 'react';
import { Button } from '../button';
import { cn } from '../utils';
import { BasePageHeaderProps, BasePageHeaderAction } from './types';

// 【2025-01-20†deduplication†base_page_header】

interface BasePageHeaderPropsWithAction extends BasePageHeaderProps {
  primaryAction?: BasePageHeaderAction & { loading?: boolean };
}

export function BasePageHeader({
  title,
  description,
  primaryAction,
  secondaryActions,
  className
}: BasePageHeaderPropsWithAction) {
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
            {Icon && (
              <Icon className={cn(
                "mr-2 h-4 w-4",
                primaryAction.loading && "animate-spin"
              )} />
            )}
            {primaryAction.label}
          </Button>
        )}
      </div>
    </div>
  );
}
