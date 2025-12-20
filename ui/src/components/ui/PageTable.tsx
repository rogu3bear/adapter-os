import React from 'react';
import { cn } from '@/lib/utils';

type PageTableWidth = 'sm' | 'md' | 'lg';

const MIN_WIDTH_MAP: Record<PageTableWidth, string> = {
  sm: '640px',
  md: '720px',
  lg: '960px',
};

interface PageTableProps {
  /** Table element (or wrapper) to render */
  children: React.ReactNode;
  /** Minimum width before horizontal scrolling kicks in */
  minWidth?: PageTableWidth;
  className?: string;
}

/**
 * PageTable ensures tables never force the page to scroll horizontally.
 * Wraps tables with overflow containment and a standard min-width contract.
 */
export function PageTable({ children, minWidth = 'md', className }: PageTableProps) {
  const minWidthValue = MIN_WIDTH_MAP[minWidth] ?? MIN_WIDTH_MAP.md;

  return (
    <div className={cn('w-full overflow-x-auto', className)} data-slot="page-table">
      <div className="inline-block min-w-full align-middle" style={{ minWidth: minWidthValue }}>
        {children}
      </div>
    </div>
  );
}

export default PageTable;

