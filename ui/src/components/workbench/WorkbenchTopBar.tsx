/**
 * WorkbenchTopBar - Status chips and export button for the Workbench
 *
 * Displays active dataset chip, active stack chip, and export button.
 */

import { Download } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ActiveDatasetChip } from './controls/ActiveDatasetChip';
import { ActiveStackChip } from './controls/ActiveStackChip';
import { cn } from '@/lib/utils';

interface WorkbenchTopBarProps {
  /** Active stack name */
  stackName?: string | null;
  /** Active stack ID */
  stackId?: string | null;
  /** Callback for export action */
  onExport?: () => void;
  /** Whether export is available */
  canExport?: boolean;
  /** Current inference latency for pulse indicator */
  latencyMs?: number | null;
  /** Additional className */
  className?: string;
}

export function WorkbenchTopBar({
  stackName,
  stackId,
  onExport,
  canExport = false,
  latencyMs,
  className,
}: WorkbenchTopBarProps) {
  return (
    <div
      className={cn(
        'flex items-center justify-between gap-4',
        className
      )}
      data-testid="workbench-top-bar"
    >
      {/* Left: Status chips */}
      <div className="flex items-center gap-2 min-w-0">
        <ActiveDatasetChip />
        <ActiveStackChip stackName={stackName} stackId={stackId} latencyMs={latencyMs} />
      </div>

      {/* Right: Export button */}
      <div className="flex items-center gap-2 flex-none">
        {onExport && (
          <Button
            variant="outline"
            size="sm"
            onClick={onExport}
            disabled={!canExport}
            className="h-7"
            data-testid="export-button"
          >
            <Download className="h-3.5 w-3.5 mr-1.5" />
            Export
          </Button>
        )}
      </div>
    </div>
  );
}
