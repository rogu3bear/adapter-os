"use client";

import * as React from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { cn } from "@/lib/utils";

interface VirtualizedTableRowsProps {
  items: unknown[];
  estimateSize?: number;
  overscan?: number;
  children: (item: unknown, index: number) => React.ReactNode;
}

/**
 * Virtualized table rows component for efficient rendering within a table.
 * 
 * Uses @tanstack/react-virtual to only render visible rows. This should be
 * used inside a TableBody that's wrapped in a scrollable container.
 * 
 * The parent table should be wrapped like:
 * ```
 * <div className="max-h-[600px] overflow-auto" data-virtual-container>
 *   <Table>
 *     <TableHeader>...</TableHeader>
 *     <TableBody>
 *       <VirtualizedTableRows items={items}>
 *         {(item) => <TableRow>...</TableRow>}
 *       </VirtualizedTableRows>
 *     </TableBody>
 *   </Table>
 * </div>
 * ```
 * 
 * @param items - Array of data items to render
 * @param estimateSize - Estimated row height in pixels (default: 60)
 * @param overscan - Number of items to render outside visible area (default: 5)
 * @param children - Render function that receives (item, index) and returns a TableRow
 */
export function VirtualizedTableRows({
  items,
  estimateSize = 60,
  overscan = 5,
  children,
}: VirtualizedTableRowsProps) {
  const parentRef = React.useRef<HTMLTableSectionElement>(null);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => {
      const container = parentRef.current?.closest('[data-virtual-container]') as HTMLElement | null;
      return container;
    },
    estimateSize: () => estimateSize,
    overscan,
  });

  if (items.length === 0) {
    return null;
  }

  const virtualItems = virtualizer.getVirtualItems();
  const totalHeight = virtualizer.getTotalSize();
  
  // Calculate padding for non-visible items
  const topOffset = virtualItems.length > 0 ? virtualItems[0]?.start ?? 0 : 0;
  const bottomOffset = virtualItems.length > 0 
    ? totalHeight - (virtualItems[virtualItems.length - 1]?.end ?? totalHeight)
    : 0;

  return (
    <>
      {topOffset > 0 && (
        <tr aria-hidden="true" style={{ height: `${topOffset}px` }}>
          <td colSpan={100} style={{ padding: 0, border: 0, lineHeight: 0 }} />
        </tr>
      )}
      {virtualItems.map((virtualRow) => {
        const item = items[virtualRow.index];
        return (
          <React.Fragment key={virtualRow.key}>
            {children(item, virtualRow.index)}
          </React.Fragment>
        );
      })}
      {bottomOffset > 0 && (
        <tr aria-hidden="true" style={{ height: `${bottomOffset}px` }}>
          <td colSpan={100} style={{ padding: 0, border: 0, lineHeight: 0 }} />
        </tr>
      )}
    </>
  );
}

interface VirtualizedListProps {
  items: unknown[];
  estimateSize?: number;
  overscan?: number;
  className?: string;
  itemClassName?: string;
  children: (item: unknown, index: number) => React.ReactNode;
}

/**
 * Virtualized list component for efficient rendering of large lists outside of tables.
 * 
 * @param items - Array of data items to render
 * @param estimateSize - Estimated item height in pixels (default: 50)
 * @param overscan - Number of items to render outside visible area (default: 5)
 * @param className - Additional CSS classes for container
 * @param itemClassName - Additional CSS classes for each item
 * @param children - Render function that receives (item, index) and returns a React node
 */
export function VirtualizedList({
  items,
  estimateSize = 50,
  overscan = 5,
  className,
  itemClassName,
  children,
}: VirtualizedListProps) {
  const parentRef = React.useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => estimateSize,
    overscan,
  });

  return (
    <div ref={parentRef} className={cn("h-full overflow-auto", className)}>
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: "100%",
          position: "relative",
        }}
      >
        {virtualizer.getVirtualItems().map((virtualItem) => {
          const item = items[virtualItem.index];
          return (
            <div
              key={virtualItem.key}
              className={itemClassName}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: `${virtualItem.size}px`,
                transform: `translateY(${virtualItem.start}px)`,
              }}
            >
              {children(item, virtualItem.index)}
            </div>
          );
        })}
      </div>
    </div>
  );
}