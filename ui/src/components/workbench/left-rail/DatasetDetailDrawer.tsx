/**
 * DatasetDetailDrawer - Sheet-based drawer for dataset details
 *
 * Shows dataset metadata and provides quick actions without leaving the workbench.
 */

import React from 'react';
import { formatDistanceToNow } from 'date-fns';
import { formatBytes } from '@/lib/formatters';
import {
  Database,
  MessageSquare,
  CheckCircle,
  Play,
  ExternalLink,
  Copy,
  Check,
} from 'lucide-react';
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
  SheetFooter,
} from '@/components/ui/sheet';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { TrustBadge } from '@/components/shared/TrustHealthBadge';
import { cn } from '@/lib/utils';
import { useNavigate } from 'react-router-dom';
import type { Dataset } from '@/api/training-types';
import { buildDatasetDetailLink } from '@/utils/navLinks';

// Validation status styles
const VALIDATION_STYLES: Record<string, { bg: string; text: string; label: string }> = {
  valid: { bg: 'bg-emerald-100', text: 'text-emerald-800', label: 'Valid' },
  validating: { bg: 'bg-blue-100', text: 'text-blue-800', label: 'Validating' },
  pending: { bg: 'bg-slate-100', text: 'text-slate-700', label: 'Pending' },
  invalid: { bg: 'bg-red-100', text: 'text-red-800', label: 'Invalid' },
  skipped: { bg: 'bg-gray-100', text: 'text-gray-700', label: 'Skipped' },
};

export interface DatasetDetailDrawerProps {
  /** The dataset to display, or null if closed */
  dataset: Dataset | null;
  /** Whether the drawer is open */
  isOpen: boolean;
  /** Called when the drawer should close */
  onClose: () => void;
  /** Called when the user clicks "Talk to this dataset" */
  onTalk: (dataset: Dataset) => void;
  /** Called when the user clicks "Validate" */
  onValidate?: (datasetId: string) => void;
  /** Called when the user clicks "Train" */
  onTrain?: (datasetId: string) => void;
}

export function DatasetDetailDrawer({
  dataset,
  isOpen,
  onClose,
  onTalk,
  onValidate,
  onTrain,
}: DatasetDetailDrawerProps) {
  const navigate = useNavigate();
  const [copiedVersion, setCopiedVersion] = React.useState(false);

  if (!dataset) return null;

  const validationStatus = dataset.validation_status ?? 'pending';
  const statusStyle = VALIDATION_STYLES[validationStatus] ?? VALIDATION_STYLES.pending;
  const canTalk = validationStatus === 'valid';
  const canValidate = validationStatus === 'pending' || validationStatus === 'invalid';
  const canTrain =
    validationStatus === 'valid' &&
    (dataset.trust_state === 'allowed' || dataset.trust_state === 'allowed_with_warning');

  const handleCopyVersion = async () => {
    if (dataset.dataset_version_id) {
      await navigator.clipboard.writeText(dataset.dataset_version_id);
      setCopiedVersion(true);
      setTimeout(() => setCopiedVersion(false), 2000);
    }
  };

  const handleViewFullDetails = () => {
    navigate(buildDatasetDetailLink(dataset.id));
    onClose();
  };

  return (
    <Sheet open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <SheetContent
        side="right"
        className="w-[400px] sm:max-w-[400px] flex flex-col"
        data-testid="dataset-detail-drawer"
      >
        <SheetHeader className="flex-none">
          <div className="flex items-center gap-2">
            <Database className="h-5 w-5 text-primary" />
            <SheetTitle className="truncate">{dataset.name}</SheetTitle>
          </div>
          {dataset.description && (
            <SheetDescription className="line-clamp-2">
              {dataset.description}
            </SheetDescription>
          )}
        </SheetHeader>

        <Separator className="my-4" />

        {/* Metadata section */}
        <div className="flex-1 overflow-y-auto space-y-4">
          {/* Version ID */}
          <div>
            <label className="text-xs font-medium text-muted-foreground">Version ID</label>
            <div className="flex items-center gap-2 mt-1">
              <code className="text-sm font-mono bg-muted px-2 py-1 rounded flex-1 truncate">
                {dataset.dataset_version_id ?? 'Not assigned'}
              </code>
              {dataset.dataset_version_id && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      onClick={handleCopyVersion}
                    >
                      {copiedVersion ? (
                        <Check className="h-4 w-4 text-green-600" />
                      ) : (
                        <Copy className="h-4 w-4" />
                      )}
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>{copiedVersion ? 'Copied!' : 'Copy version ID'}</TooltipContent>
                </Tooltip>
              )}
            </div>
          </div>

          {/* Status badges */}
          <div className="flex flex-wrap gap-3">
            <div>
              <label className="text-xs font-medium text-muted-foreground block mb-1">
                Validation
              </label>
              <Badge
                variant="outline"
                className={cn('text-xs', statusStyle.bg, statusStyle.text)}
              >
                {statusStyle.label}
              </Badge>
            </div>
            <div>
              <label className="text-xs font-medium text-muted-foreground block mb-1">
                Trust
              </label>
              <TrustBadge state={dataset.trust_state} reason={dataset.trust_reason} size="sm" />
            </div>
          </div>

          {/* Updated time */}
          <div>
            <label className="text-xs font-medium text-muted-foreground">Last Updated</label>
            <p className="text-sm mt-1">
              {formatDistanceToNow(new Date(dataset.updated_at), { addSuffix: true })}
            </p>
          </div>

          {/* File stats */}
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-muted-foreground">Files</label>
              <p className="text-sm mt-1">{dataset.file_count.toLocaleString()}</p>
            </div>
            <div>
              <label className="text-xs font-medium text-muted-foreground">Size</label>
              <p className="text-sm mt-1">{formatBytes(dataset.total_size_bytes)}</p>
            </div>
          </div>
        </div>

        <Separator className="my-4" />

        {/* Actions */}
        <SheetFooter className="flex-none flex-col gap-2 sm:flex-col">
          {/* Primary CTA: Talk */}
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                className="w-full"
                size="lg"
                onClick={() => onTalk(dataset)}
                disabled={!canTalk}
                data-testid="drawer-talk-button"
              >
                <MessageSquare className="h-4 w-4 mr-2" />
                Talk to this dataset
              </Button>
            </TooltipTrigger>
            {!canTalk && (
              <TooltipContent>Dataset must be validated before chatting</TooltipContent>
            )}
          </Tooltip>

          {/* Secondary actions */}
          <div className="flex gap-2 w-full">
            {canValidate && onValidate && (
              <Button
                variant="outline"
                className="flex-1"
                onClick={() => onValidate(dataset.id)}
                data-testid="drawer-validate-button"
              >
                <CheckCircle className="h-4 w-4 mr-2" />
                Validate
              </Button>
            )}
            {canTrain && onTrain && (
              <Button
                variant="outline"
                className="flex-1"
                onClick={() => onTrain(dataset.id)}
                data-testid="drawer-train-button"
              >
                <Play className="h-4 w-4 mr-2" />
                Train
              </Button>
            )}
          </div>

          {/* View full details link */}
          <Button
            variant="ghost"
            className="w-full text-muted-foreground"
            onClick={handleViewFullDetails}
          >
            <ExternalLink className="h-4 w-4 mr-2" />
            View full details
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}

export default DatasetDetailDrawer;
