import React from 'react';
import { Alert, AlertDescription, AlertTitle } from './alert';
import { Button } from './button';
import { X, Lightbulb } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface ProgressiveHintProps {
  title: string;
  content: string;
  placement?: 'top' | 'bottom' | 'left' | 'right';
  onDismiss: () => void;
  className?: string;
  variant?: 'default' | 'info' | 'warning' | 'tip';
}

export function ProgressiveHint({
  title,
  content,
  onDismiss,
  className,
  variant = 'tip',
  placement = 'top'
}: ProgressiveHintProps) {
  const variantStyles = {
    default: 'border-border bg-background',
    info: 'border-blue-200 bg-blue-50 dark:bg-blue-950',
    warning: 'border-amber-200 bg-amber-50 dark:bg-amber-950',
    tip: 'border-primary/20 bg-primary/5'
  };

  return (
    <Alert className={cn(variantStyles[variant], className)}>
      <Lightbulb className="h-4 w-4" />
      <AlertTitle className="flex items-center justify-between">
        <span>{title}</span>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0"
          onClick={onDismiss}
          aria-label="Dismiss hint"
        >
          <X className="h-3 w-3" />
        </Button>
      </AlertTitle>
      <AlertDescription className="mt-2">
        {content}
      </AlertDescription>
    </Alert>
  );
}

