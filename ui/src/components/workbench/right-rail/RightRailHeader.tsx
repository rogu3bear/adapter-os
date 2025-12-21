/**
 * RightRailHeader - Header for the right rail with pin and collapse controls
 *
 * Shows the title, selected message timestamp, pin toggle, and collapse button.
 */

import { Pin, PinOff, ChevronRight, ChevronLeft } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useWorkbench } from '@/contexts/WorkbenchContext';
import { cn } from '@/lib/utils';
import { formatDistanceToNow } from 'date-fns';

interface RightRailHeaderProps {
  /** Title to display */
  title?: string;
  /** Timestamp of the selected message */
  selectedMessageTimestamp?: Date | null;
}

export function RightRailHeader({
  title = 'Trace',
  selectedMessageTimestamp,
}: RightRailHeaderProps) {
  const {
    rightRailCollapsed,
    toggleRightRail,
    pinnedMessageId,
    pinMessage,
    selectedMessageId,
  } = useWorkbench();

  const isPinned = pinnedMessageId !== null;

  const handleTogglePin = () => {
    if (isPinned) {
      // Unpin
      pinMessage(null);
    } else if (selectedMessageId) {
      // Pin current selection
      pinMessage(selectedMessageId);
    }
  };

  return (
    <div
      className={cn(
        'flex items-center justify-between border-b bg-background px-3 py-2',
        'h-12 flex-none'
      )}
      data-testid="right-rail-header"
    >
      <div className="flex items-center gap-2 min-w-0">
        <span className="font-medium text-sm">{title}</span>
        {selectedMessageTimestamp && (
          <span className="text-xs text-muted-foreground truncate">
            {formatDistanceToNow(selectedMessageTimestamp, { addSuffix: true })}
          </span>
        )}
      </div>

      <div className="flex items-center gap-1">
        {/* Pin toggle */}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={handleTogglePin}
              disabled={!selectedMessageId && !isPinned}
              data-testid="pin-toggle-button"
            >
              {isPinned ? (
                <Pin className="h-4 w-4 text-primary" />
              ) : (
                <PinOff className="h-4 w-4" />
              )}
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            {isPinned ? 'Auto-update disabled (click to enable)' : 'Pin to stop auto-update'}
          </TooltipContent>
        </Tooltip>

        {/* Collapse toggle */}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={toggleRightRail}
              data-testid="collapse-toggle-button"
            >
              {rightRailCollapsed ? (
                <ChevronLeft className="h-4 w-4" />
              ) : (
                <ChevronRight className="h-4 w-4" />
              )}
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            {rightRailCollapsed ? 'Expand panel' : 'Collapse panel'}
          </TooltipContent>
        </Tooltip>
      </div>
    </div>
  );
}
