/**
 * ActiveStackChip - Shows the active adapter stack name
 *
 * Displays the currently active stack. Clicking navigates to Stacks tab.
 */

import { useMemo } from 'react';
import { Layers } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { useWorkbench } from '@/contexts/WorkbenchContext';
import { cn } from '@/lib/utils';

interface ActiveStackChipProps {
  /** Stack name to display */
  stackName?: string | null;
  /** Stack ID */
  stackId?: string | null;
  /** Latest latency measurement to drive pulse */
  latencyMs?: number | null;
  /** Additional className */
  className?: string;
}

export function ActiveStackChip({
  stackName,
  stackId,
  latencyMs,
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

  const pulseDuration = useMemo(() => {
    if (!latencyMs || latencyMs <= 0) return 1.2;
    const seconds = latencyMs / 700;
    return Math.min(Math.max(seconds, 0.6), 3);
  }, [latencyMs]);

  const latencyLabel = useMemo(() => {
    if (!latencyMs || latencyMs <= 0) return null;
    return `${latencyMs.toFixed(1)}ms`;
  }, [latencyMs]);

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
      title={latencyLabel ?? undefined}
    >
      <span
        className="relative flex h-3 w-3 items-center justify-center"
        aria-hidden="true"
      >
        <span
          className={cn(
            'h-2 w-2 rounded-full bg-emerald-500 animate-pulse',
            latencyMs ? 'shadow-[0_0_0_6px_rgba(16,185,129,0.25)]' : 'opacity-70'
          )}
          style={{ animationDuration: `${pulseDuration}s` }}
        />
      </span>
      <Layers className="h-3.5 w-3.5" />
      <span className="text-xs font-medium max-w-[120px] truncate">
        {stackName}
      </span>
    </Badge>
  );
}
