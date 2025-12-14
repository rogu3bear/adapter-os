import React from 'react';
import { LucideIcon } from 'lucide-react';
import { Card, CardContent } from './card';
import { Button } from './button';
import { cn } from './utils';

type EmptyStateVariant = 'card' | 'minimal';

interface EmptyStateProps {
  icon: LucideIcon;
  title: string;
  description: string;
  actionLabel?: string;
  onAction?: () => void;
  ariaLabel?: string;
  variant?: EmptyStateVariant;
  testId?: string;
  className?: string;
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
  ariaLabel,
  variant = 'card',
  testId,
  className,
}: EmptyStateProps) {
  const resolvedLabel = ariaLabel || `${title}: ${description}`;

  if (variant === 'minimal') {
    return (
      <div
        className={cn('py-8 text-center text-muted-foreground', className)}
        role="status"
        aria-label={resolvedLabel}
        data-testid={testId ?? 'empty-state'}
      >
        <Icon className="mx-auto mb-2 h-8 w-8 opacity-50" aria-hidden="true" />
        <p>{title}</p>
        <p className="mt-1 text-sm">{description}</p>
        {actionLabel && onAction && (
          <div className="mt-4">
            <Button onClick={onAction} variant="default">
              {actionLabel}
            </Button>
          </div>
        )}
      </div>
    );
  }

  return (
    <Card
      className={cn('border-dashed', className)}
      role="status"
      aria-label={resolvedLabel}
      data-testid={testId ?? 'empty-state'}
    >
      <CardContent className="flex flex-col items-center justify-center py-12 text-center">
        <div className="flex-center mb-4">
          <Icon className="h-12 w-12 text-muted-foreground opacity-50" aria-hidden="true" />
        </div>
        <h3 className="text-lg font-semibold text-foreground mb-2">{title}</h3>
        <p className="text-sm text-muted-foreground mb-4 max-w-md">{description}</p>
        {actionLabel && onAction && (
          <Button onClick={onAction} variant="default">
            {actionLabel}
          </Button>
        )}
      </CardContent>
    </Card>
  );
}
