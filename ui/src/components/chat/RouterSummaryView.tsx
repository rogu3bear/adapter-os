/**
 * RouterSummaryView - Non-technical summary of router decision
 *
 * Provides a user-friendly explanation of why specific adapters were chosen,
 * hiding technical details behind the "Technical Proof" tab.
 */

import React from 'react';
import { CheckCircle, Info } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Button } from '@/components/ui/button';
import { getFriendlyTerm, getTermDescription, LIFECYCLE_STATE_LABELS } from '@/constants/terminology';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';

export interface RouterDecisionSummary {
  /** Overall confidence (0-100) */
  confidence: number;
  /** Selected adapters */
  selectedAdapters: Array<{
    id: string;
    name: string;
    score: number;
    state?: string;
  }>;
  /** Why these adapters were selected */
  reasoning?: string;
  /** Timestamp of decision */
  timestamp: string;
}

interface RouterSummaryViewProps {
  decision: RouterDecisionSummary;
  onExportAudit?: () => void;
}

function ConfidenceIndicator({ value }: { value: number }) {
  const getColor = (v: number) => {
    if (v >= 80) return 'bg-green-500';
    if (v >= 50) return 'bg-yellow-500';
    return 'bg-red-500';
  };

  const getLabel = (v: number) => {
    if (v >= 80) return 'High confidence';
    if (v >= 50) return 'Medium confidence';
    return 'Low confidence';
  };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="space-y-1 cursor-default">
          <div className="flex justify-between text-sm">
            <span>{getFriendlyTerm('entropy')}</span>
            <span className="font-medium">{value}%</span>
          </div>
          <Progress value={value} className={getColor(value)} />
        </div>
      </TooltipTrigger>
      <TooltipContent>
        <p>{getLabel(value)}</p>
        <p className="text-xs text-muted-foreground">
          {getTermDescription('entropy')}
        </p>
      </TooltipContent>
    </Tooltip>
  );
}

export default function RouterSummaryView({
  decision,
  onExportAudit,
}: RouterSummaryViewProps) {
  return (
    <div className="space-y-4">
      {/* Confidence Overview */}
      <div className="p-4 rounded-lg bg-muted/30">
        <ConfidenceIndicator value={decision.confidence} />
      </div>

      {/* Selected Adapters */}
      <div>
        <h4 className="text-sm font-medium mb-2 flex items-center gap-2">
          <CheckCircle className="h-4 w-4 text-green-500" />
          Selected Adapters
        </h4>
        <div className="space-y-2">
          {decision.selectedAdapters.map((adapter) => (
            <div
              key={adapter.id}
              className="flex items-center justify-between p-2 rounded border bg-background"
            >
              <div className="flex items-center gap-2">
                <span className="font-medium">{adapter.name}</span>
                {adapter.state && (
                  <Badge variant="outline" className="text-xs">
                    {LIFECYCLE_STATE_LABELS[adapter.state] || adapter.state}
                  </Badge>
                )}
              </div>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Badge variant="secondary" className="cursor-default">
                    {Math.round(adapter.score * 100)}%
                  </Badge>
                </TooltipTrigger>
                <TooltipContent>
                  <p>Match score for this adapter</p>
                </TooltipContent>
              </Tooltip>
            </div>
          ))}
        </div>
      </div>

      {/* Reasoning */}
      {decision.reasoning && (
        <div className="p-3 rounded-lg bg-blue-50 border border-blue-200">
          <div className="flex gap-2">
            <Info className="h-4 w-4 text-blue-500 mt-0.5 flex-shrink-0" />
            <p className="text-sm text-blue-700">{decision.reasoning}</p>
          </div>
        </div>
      )}

      {/* Actions */}
      <div className="flex justify-end pt-2">
        {onExportAudit && (
          <Button variant="outline" size="sm" onClick={onExportAudit}>
            Export Audit Event
          </Button>
        )}
      </div>

      {/* Timestamp */}
      <div className="text-xs text-muted-foreground text-right">
        Decision made at {new Date(decision.timestamp).toLocaleString()}
      </div>
    </div>
  );
}
