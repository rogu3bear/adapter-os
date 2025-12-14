/**
 * RightRail - Collapsible container for Evidence/Trace panel
 *
 * Shows trace and evidence information for the selected chat message.
 * Auto-updates to latest message unless pinned.
 */

import { ReactNode } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { RightRailHeader } from './RightRailHeader';
import { useWorkbench } from '@/contexts/WorkbenchContext';

interface RightRailProps {
  /** Content to display in the rail */
  children: ReactNode;
  /** Title for the header */
  title?: string;
  /** Timestamp of the selected message */
  selectedMessageTimestamp?: Date | null;
}

export function RightRail({
  children,
  title = 'Trace',
  selectedMessageTimestamp,
}: RightRailProps) {
  return (
    <div className="flex h-full flex-col" data-testid="right-rail">
      <RightRailHeader
        title={title}
        selectedMessageTimestamp={selectedMessageTimestamp}
      />
      <ScrollArea className="flex-1">
        <div className="p-3">{children}</div>
      </ScrollArea>
    </div>
  );
}

/**
 * RightRailToggle - Floating button to expand collapsed right rail
 *
 * Shows when the right rail is collapsed, allowing users to expand it.
 */
export function RightRailToggle() {
  const { rightRailCollapsed, toggleRightRail } = useWorkbench();

  if (!rightRailCollapsed) return null;

  return (
    <button
      className="fixed right-4 top-1/2 -translate-y-1/2 z-40 flex items-center justify-center w-8 h-16 rounded-l-md bg-muted border border-r-0 shadow-sm hover:bg-accent transition-colors"
      onClick={toggleRightRail}
      aria-label="Expand trace panel"
      data-testid="right-rail-toggle"
    >
      <span className="text-xs font-medium [writing-mode:vertical-lr] rotate-180">
        Trace
      </span>
    </button>
  );
}
