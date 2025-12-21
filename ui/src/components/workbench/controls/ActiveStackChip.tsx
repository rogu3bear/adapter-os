/**
 * ActiveStackChip - Shows the active adapter stack name
 *
 * Displays the currently active stack. Clicking navigates to Stacks tab.
 */

import { Layers } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { useWorkbench } from '@/contexts/WorkbenchContext';
import { cn } from '@/lib/utils';

interface ActiveStackChipProps {
  /** Stack name to display */
  stackName?: string | null;
  /** Stack ID */
  stackId?: string | null;
  /** Additional className */
  className?: string;
}

export function ActiveStackChip({
  stackName,
  stackId,
  className,
}: ActiveStackChipProps) {
  const { setActiveLeftTab } = useWorkbench();

  if (!stackId || !stackName) {
    return null;
  }

  const handleClick = () => {
    // Navigate to Stacks tab when clicked
    setActiveLeftTab('stacks');
  };

  return (
    <Badge
      variant="outline"
      className={cn(
        'flex items-center gap-1.5 px-2 py-1 h-7 cursor-pointer hover:bg-muted transition-colors',
        className
      )}
      onClick={handleClick}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          handleClick();
        }
      }}
      data-testid="active-stack-chip"
    >
      <Layers className="h-3.5 w-3.5" />
      <span className="text-xs font-medium max-w-[120px] truncate">
        {stackName}
      </span>
    </Badge>
  );
}
