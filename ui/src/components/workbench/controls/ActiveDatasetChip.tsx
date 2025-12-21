/**
 * ActiveDatasetChip - Shows the active dataset scope with clear button
 *
 * Displays the currently scoped dataset name, version ID, validation status,
 * and allows clearing it with one click.
 */

import { Database, X, CheckCircle, AlertCircle, Clock, Loader2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useDatasetChatOptional } from '@/contexts/DatasetChatContext';
import { useTraining } from '@/hooks/training';
import { cn } from '@/lib/utils';

// Validation status icon mapping
const STATUS_ICONS: Record<string, { icon: typeof CheckCircle; className: string }> = {
  valid: { icon: CheckCircle, className: 'text-emerald-600' },
  validating: { icon: Loader2, className: 'text-blue-600 animate-spin' },
  draft: { icon: Clock, className: 'text-slate-500' },
  invalid: { icon: AlertCircle, className: 'text-red-600' },
  failed: { icon: AlertCircle, className: 'text-red-600' },
};

interface ActiveDatasetChipProps {
  /** Additional className */
  className?: string;
}

export function ActiveDatasetChip({ className }: ActiveDatasetChipProps) {
  const datasetContext = useDatasetChatOptional();
  const { useDataset } = useTraining;

  // Fetch dataset details for validation status
  const { data: dataset } = useDataset(datasetContext?.activeDatasetId ?? '', {
    enabled: !!datasetContext?.activeDatasetId,
  });

  if (!datasetContext?.activeDatasetId) {
    return null;
  }

  const handleClear = () => {
    datasetContext.clearActiveDataset();
  };

  const versionId = datasetContext.datasetVersionId;
  const validationStatus = dataset?.validation_status ?? 'draft';
  const statusConfig = STATUS_ICONS[validationStatus] ?? STATUS_ICONS.draft;
  const StatusIcon = statusConfig.icon;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge
          variant="secondary"
          className={cn(
            'flex items-center gap-1.5 pl-2 pr-1 py-1 h-auto min-h-[28px]',
            className
          )}
          data-testid="active-dataset-chip"
        >
          <Database className="h-3.5 w-3.5 flex-shrink-0" />
          <div className="flex flex-col items-start min-w-0">
            <span className="text-xs font-medium max-w-[100px] truncate leading-tight">
              {datasetContext.activeDatasetName}
            </span>
            {versionId && (
              <span className="text-[10px] text-muted-foreground font-mono leading-tight">
                v:{versionId.slice(0, 8)}
              </span>
            )}
          </div>
          <StatusIcon className={cn('h-3.5 w-3.5 flex-shrink-0 ml-0.5', statusConfig.className)} />
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5 rounded-full hover:bg-muted flex-shrink-0 ml-0.5"
            onClick={handleClear}
            data-testid="clear-dataset-chip"
          >
            <X className="h-3 w-3" />
          </Button>
        </Badge>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="text-xs">
        <div className="space-y-1">
          <div className="font-medium">{datasetContext.activeDatasetName}</div>
          {versionId && (
            <div className="text-muted-foreground font-mono">Version: {versionId}</div>
          )}
          <div className="text-muted-foreground capitalize">Status: {validationStatus}</div>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

export default ActiveDatasetChip;
