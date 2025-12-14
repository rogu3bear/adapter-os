import React from 'react';
import { Loader2 } from 'lucide-react';
import { Skeleton } from './skeleton';
import { cn } from './utils';

type LoadingStateSize = 'sm' | 'md';
type LoadingStateVariant = 'card' | 'minimal';

const sizeClassMap: Record<LoadingStateSize, string> = {
  sm: 'p-4',
  md: 'p-8',
};

const spinnerClassMap: Record<LoadingStateSize, string> = {
  sm: 'h-5 w-5',
  md: 'h-6 w-6',
};

const skeletonHeightMap: Record<LoadingStateSize, string> = {
  sm: 'h-3',
  md: 'h-4',
};

interface LoadingStateProps {
  title?: string;
  description?: string;
  message?: string;
  skeletonLines?: number;
  size?: LoadingStateSize;
  variant?: LoadingStateVariant;
  testId?: string;
  ariaLabel?: string;
  className?: string;
}

export function LoadingState({
  title,
  description,
  message,
  skeletonLines = 0,
  size = 'md',
  variant = 'card',
  testId,
  ariaLabel,
  className,
}: LoadingStateProps) {
  const resolvedLabel = ariaLabel || title || message || 'Loading';

  if (variant === 'minimal') {
    return (
      <div
        className={cn('py-8 text-center text-muted-foreground', className)}
        role="status"
        aria-live="polite"
        aria-label={resolvedLabel}
        data-testid={testId ?? 'loading-state'}
      >
        <Loader2 className={cn('mx-auto animate-spin', spinnerClassMap[size])} aria-hidden="true" />
        {title && <p className="mt-2 text-sm">{title}</p>}
        {description && <p className="mt-1 text-sm">{description}</p>}
        {message && <p className="mt-2 text-sm">{message}</p>}
        {skeletonLines > 0 && (
          <div className="mt-4 w-full space-y-2">
            {Array.from({ length: skeletonLines }).map((_, index) => (
              <Skeleton key={index} className={cn('w-full', skeletonHeightMap[size])} />
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center rounded-lg border border-dashed border-border bg-card/40 text-center shadow-sm',
        sizeClassMap[size],
        className,
      )}
      role="status"
      aria-live="polite"
      aria-label={resolvedLabel}
      data-testid={testId ?? 'loading-state'}
    >
      <Loader2 className={cn('animate-spin text-primary', spinnerClassMap[size])} aria-hidden="true" />
      {title && <h3 className="mt-3 text-sm font-medium text-foreground">{title}</h3>}
      {description && <p className="mt-1 text-sm text-muted-foreground">{description}</p>}
      {message && <p className="mt-1 text-sm text-muted-foreground">{message}</p>}
      {skeletonLines > 0 && (
        <div className="mt-4 w-full space-y-2">
          {Array.from({ length: skeletonLines }).map((_, index) => (
            <Skeleton key={index} className={cn('w-full', skeletonHeightMap[size])} />
          ))}
        </div>
      )}
    </div>
  );
}
