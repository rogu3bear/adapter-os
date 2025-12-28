import { AlertCircle } from 'lucide-react';
import { cn } from '@/lib/utils';

interface UnavailableMetricProps {
  /** Label for the unavailable metric */
  label: string;
  /** Optional additional class names */
  className?: string;
  /** Size variant */
  size?: 'sm' | 'md' | 'lg';
}

/**
 * Component to display when a metric is unavailable.
 *
 * Used instead of showing zeros when telemetry data cannot be collected.
 * This prevents operators from making decisions based on false data.
 */
export function UnavailableMetric({
  label,
  className,
  size = 'md',
}: UnavailableMetricProps) {
  const sizeClasses = {
    sm: 'text-xs gap-1',
    md: 'text-sm gap-2',
    lg: 'text-base gap-2',
  };

  const iconSizes = {
    sm: 'h-3 w-3',
    md: 'h-4 w-4',
    lg: 'h-5 w-5',
  };

  return (
    <div
      className={cn(
        'flex items-center text-muted-foreground',
        sizeClasses[size],
        className
      )}
    >
      <AlertCircle className={cn(iconSizes[size], 'flex-shrink-0')} />
      <span>{label} Unavailable</span>
    </div>
  );
}

/**
 * Helper function to check if a metric is available.
 *
 * Use this to determine whether to show actual data or the UnavailableMetric component.
 */
export function isMetricAvailable(
  availability: 'available' | 'unavailable' | 'stale' | undefined | null
): boolean {
  return availability === 'available';
}

/**
 * Helper function to check if a metric is stale.
 */
export function isMetricStale(
  availability: 'available' | 'unavailable' | 'stale' | undefined | null
): boolean {
  return availability === 'stale';
}
