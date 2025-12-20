import React from 'react';
import { cn } from '@/lib/utils';

interface ResponsiveStatGridProps {
  children: React.ReactNode;
  className?: string;
}

/**
 * Responsive grid helper for analytics/stat cards.
 * - 1 column on small screens
 * - 2 columns on medium
 * - 3 columns on extra-large
 */
export function ResponsiveStatGrid({ children, className }: ResponsiveStatGridProps) {
  return (
    <div className={cn('grid gap-4 sm:grid-cols-1 md:grid-cols-2 xl:grid-cols-3', className)}>
      {children}
    </div>
  );
}

export default ResponsiveStatGrid;

