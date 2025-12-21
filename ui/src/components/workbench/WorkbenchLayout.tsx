/**
 * WorkbenchLayout - Three-column layout for the Workbench
 *
 * Layout structure:
 * - Left rail (320px): Tabbed navigation (Sessions | Datasets | Stacks)
 * - Center (flex-1): ChatInterface
 * - Right rail (384px, collapsible): Evidence/Trace panel
 */

import { ReactNode } from 'react';
import { cn } from '@/lib/utils';
import { useWorkbench } from '@/contexts/WorkbenchContext';

interface WorkbenchLayoutProps {
  /** Left rail content */
  leftRail: ReactNode;
  /** Center content (ChatInterface) */
  center: ReactNode;
  /** Right rail content */
  rightRail: ReactNode;
  /** Optional top bar content */
  topBar?: ReactNode;
  /** Additional className */
  className?: string;
}

export function WorkbenchLayout({
  leftRail,
  center,
  rightRail,
  topBar,
  className,
}: WorkbenchLayoutProps) {
  const { rightRailCollapsed } = useWorkbench();

  return (
    <div
      className={cn('flex h-full flex-col', className)}
      data-testid="workbench-layout"
    >
      {/* Top bar */}
      {topBar && (
        <div className="flex-none border-b bg-background px-4 py-2">
          {topBar}
        </div>
      )}

      {/* Main content area */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Left rail - fixed width */}
        <aside
          className="flex-none w-80 border-r bg-muted/30 overflow-hidden"
          data-testid="workbench-left-rail"
        >
          {leftRail}
        </aside>

        {/* Center - flexible */}
        <main
          className="flex-1 min-w-0 overflow-hidden"
          data-testid="workbench-center"
        >
          {center}
        </main>

        {/* Right rail - collapsible */}
        <aside
          className={cn(
            'flex-none border-l bg-muted/30 overflow-hidden transition-all duration-200 ease-in-out',
            rightRailCollapsed ? 'w-0' : 'w-96'
          )}
          data-testid="workbench-right-rail"
        >
          <div
            className={cn(
              'h-full w-96 transition-opacity duration-200',
              rightRailCollapsed ? 'opacity-0' : 'opacity-100'
            )}
          >
            {rightRail}
          </div>
        </aside>
      </div>
    </div>
  );
}
