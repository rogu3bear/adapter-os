/**
 * DatasetCard - Folder-like card for displaying a dataset in the grid view
 *
 * Shows dataset name, validation status, version, and provides quick Talk action.
 */

import React from 'react';
import { FolderOpen, MessageSquare, Check } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import type { Dataset } from '@/api/training-types';

// Validation status colors
const VALIDATION_STYLES: Record<string, { bg: string; text: string; label: string }> = {
  valid: { bg: 'bg-emerald-100', text: 'text-emerald-800', label: 'Valid' },
  validating: { bg: 'bg-blue-100', text: 'text-blue-800', label: 'Validating' },
  draft: { bg: 'bg-slate-100', text: 'text-slate-700', label: 'Draft' },
  invalid: { bg: 'bg-red-100', text: 'text-red-800', label: 'Invalid' },
  failed: { bg: 'bg-red-100', text: 'text-red-800', label: 'Failed' },
};

export interface DatasetCardProps {
  /** The dataset to display */
  dataset: Dataset;
  /** Whether this dataset is currently active/selected */
  isActive: boolean;
  /** Called when the card is clicked (opens drawer) */
  onSelect: () => void;
  /** Called when the Talk button is clicked */
  onTalk: () => void;
}

export function DatasetCard({ dataset, isActive, onSelect, onTalk }: DatasetCardProps) {
  const validationStatus = dataset.validation_status ?? 'draft';
  const statusStyle = VALIDATION_STYLES[validationStatus] ?? VALIDATION_STYLES.draft;
  const versionId = dataset.dataset_version_id;
  const canTalk = validationStatus === 'valid';

  const handleTalkClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (canTalk) {
      onTalk();
    }
  };

  return (
    <Card
      className={cn(
        'relative p-3 cursor-pointer transition-all hover:shadow-md',
        'border hover:border-primary/50',
        isActive && 'border-primary bg-primary/5 ring-1 ring-primary/20'
      )}
      onClick={onSelect}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect();
        }
      }}
      data-testid={`dataset-card-${dataset.id}`}
    >
      {/* Active indicator */}
      {isActive && (
        <div className="absolute top-2 right-2">
          <Check className="h-4 w-4 text-primary" />
        </div>
      )}

      {/* Folder icon and name */}
      <div className="flex items-start gap-2 mb-2">
        <FolderOpen className="h-5 w-5 text-amber-500 flex-shrink-0 mt-0.5" />
        <div className="min-w-0 flex-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <h4 className="font-medium text-sm truncate pr-4">{dataset.name}</h4>
            </TooltipTrigger>
            <TooltipContent side="top">{dataset.name}</TooltipContent>
          </Tooltip>
        </div>
      </div>

      {/* Status and version badges */}
      <div className="flex flex-wrap gap-1.5 mb-3">
        <Badge
          variant="outline"
          className={cn('text-[10px] px-1.5 py-0 h-5', statusStyle.bg, statusStyle.text)}
        >
          {statusStyle.label}
        </Badge>
        {versionId && (
          <Badge variant="outline" className="text-[10px] px-1.5 py-0 h-5 font-mono">
            v:{versionId.slice(0, 8)}
          </Badge>
        )}
      </div>

      {/* Talk button */}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant={canTalk ? 'default' : 'secondary'}
            size="sm"
            className="w-full h-7 text-xs"
            onClick={handleTalkClick}
            disabled={!canTalk}
            data-testid="dataset-card-talk"
          >
            <MessageSquare className="h-3.5 w-3.5 mr-1.5" />
            Talk
          </Button>
        </TooltipTrigger>
        {!canTalk && (
          <TooltipContent side="top">
            Dataset must be validated before chatting
          </TooltipContent>
        )}
      </Tooltip>
    </Card>
  );
}

export default DatasetCard;
