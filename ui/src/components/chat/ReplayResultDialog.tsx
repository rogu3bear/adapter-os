/**
 * Replay Result Dialog Component
 *
 * Displays replay execution results with comparison, details, and statistics.
 * Part of PRD-02 Deterministic Replay feature.
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import React, { useState, useMemo } from 'react';
import {
  CheckCircle2,
  XCircle,
  AlertCircle,
  AlertTriangle,
  Copy,
  Clock,
  Zap,
  Database,
  TrendingUp,
  ArrowRight,
  History,
} from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';
import type {
  ReplayResponse,
  ReplayMatchStatus,
} from '@/api/replay-types';
import {
  getMatchStatusLabel,
  getMatchStatusBadgeVariant,
  getRagReproducibilityPercent,
  formatLatencyDiff,
} from '@/api/replay-types';

interface ReplayResultDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  replayResponse: ReplayResponse | null;
  onViewHistory?: () => void;
}

/**
 * Get badge variant and color for match status
 */
function getMatchStatusBadge(status: ReplayMatchStatus): {
  variant: 'success' | 'warning' | 'error' | 'neutral';
  icon: React.ReactNode;
} {
  switch (status) {
    case 'exact':
      return {
        variant: 'success',
        icon: <CheckCircle2 className="size-3" />,
      };
    case 'semantic':
      return {
        variant: 'warning',
        icon: <AlertCircle className="size-3" />,
      };
    case 'divergent':
      return {
        variant: 'error',
        icon: <TrendingUp className="size-3" />,
      };
    case 'error':
      return {
        variant: 'error',
        icon: <XCircle className="size-3" />,
      };
    default:
      return {
        variant: 'neutral',
        icon: <AlertCircle className="size-3" />,
      };
  }
}

/**
 * Copy text to clipboard with toast notification
 */
function copyToClipboard(text: string, label: string) {
  navigator.clipboard.writeText(text).then(
    () => {
      toast.success(`${label} copied to clipboard`);
    },
    (err) => {
      toast.error(`Failed to copy: ${err}`);
    }
  );
}

/**
 * Response Comparison Component
 * Shows side-by-side or diff view of original vs replay
 */
function ResponseComparison({
  original,
  replay,
  divergencePosition,
}: {
  original: string;
  replay: string;
  divergencePosition?: number;
}) {
  const [viewMode, setViewMode] = useState<'side-by-side' | 'diff'>('side-by-side');

  // Highlight divergence position if provided
  const highlightedOriginal = useMemo(() => {
    if (divergencePosition === undefined) return original;
    const before = original.slice(0, divergencePosition);
    const after = original.slice(divergencePosition);
    return (
      <>
        {before}
        <span className="bg-red-200 dark:bg-red-900/40 border-b-2 border-red-500">
          {after}
        </span>
      </>
    );
  }, [original, divergencePosition]);

  const highlightedReplay = useMemo(() => {
    if (divergencePosition === undefined) return replay;
    const before = replay.slice(0, divergencePosition);
    const after = replay.slice(divergencePosition);
    return (
      <>
        {before}
        <span className="bg-yellow-200 dark:bg-yellow-900/40 border-b-2 border-yellow-500">
          {after}
        </span>
      </>
    );
  }, [replay, divergencePosition]);

  if (viewMode === 'side-by-side') {
    return (
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h4 className="text-sm font-medium">Comparison</h4>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setViewMode('diff')}
          >
            Show Diff View
          </Button>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-muted-foreground">
                Original Response
              </span>
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => copyToClipboard(original, 'Original response')}
              >
                <Copy className="size-3" />
              </Button>
            </div>
            <div className="bg-muted/50 rounded-md p-3 text-sm font-mono whitespace-pre-wrap break-words max-h-[400px] overflow-y-auto">
              {divergencePosition !== undefined ? highlightedOriginal : original}
            </div>
          </div>
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-muted-foreground">
                Replay Response
              </span>
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => copyToClipboard(replay, 'Replay response')}
              >
                <Copy className="size-3" />
              </Button>
            </div>
            <div className="bg-muted/50 rounded-md p-3 text-sm font-mono whitespace-pre-wrap break-words max-h-[400px] overflow-y-auto">
              {divergencePosition !== undefined ? highlightedReplay : replay}
            </div>
          </div>
        </div>
        {divergencePosition !== undefined && (
          <div className="text-xs text-muted-foreground">
            <AlertTriangle className="inline-block size-3 mr-1" />
            Divergence detected at character position {divergencePosition}
          </div>
        )}
      </div>
    );
  }

  // Diff view (simple character-by-character comparison)
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium">Diff View</h4>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setViewMode('side-by-side')}
        >
          Show Side-by-Side
        </Button>
      </div>
      <div className="bg-muted/50 rounded-md p-3 text-sm font-mono whitespace-pre-wrap break-words max-h-[400px] overflow-y-auto">
        {original === replay ? (
          <div className="text-green-600 dark:text-green-400">
            <CheckCircle2 className="inline-block size-4 mr-2" />
            Responses are identical
          </div>
        ) : (
          <div className="space-y-2">
            <div className="text-red-600 dark:text-red-400">
              <XCircle className="inline-block size-4 mr-2" />
              - Original
            </div>
            <div className="pl-4 text-red-600/80 dark:text-red-400/80">
              {original}
            </div>
            <div className="text-green-600 dark:text-green-400 mt-4">
              <CheckCircle2 className="inline-block size-4 mr-2" />
              + Replay
            </div>
            <div className="pl-4 text-green-600/80 dark:text-green-400/80">
              {replay}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Details Tab Component
 * Shows match status, divergence details, and approximation reasons
 */
function DetailsTab({ replayResponse }: { replayResponse: ReplayResponse }) {
  const { match_status, divergence, replay_mode } = replayResponse;
  const badge = getMatchStatusBadge(match_status);

  return (
    <div className="space-y-4">
      {/* Match Status */}
      <div className="space-y-2">
        <h4 className="text-sm font-medium">Match Status</h4>
        <div className="flex items-center gap-2">
          <Badge variant={badge.variant}>
            {badge.icon}
            {getMatchStatusLabel(match_status)}
          </Badge>
          <Badge variant="outline">{replay_mode}</Badge>
        </div>
      </div>

      {/* Divergence Details */}
      {divergence && (
        <div className="space-y-2">
          <h4 className="text-sm font-medium">Divergence Analysis</h4>
          <div className="bg-muted/50 rounded-md p-3 space-y-2 text-sm">
            {divergence.divergence_position !== undefined && (
              <div className="flex items-start gap-2">
                <AlertTriangle className="size-4 text-orange-500 mt-0.5" />
                <div>
                  <span className="font-medium">Divergence Position:</span>
                  <span className="ml-2">Character {divergence.divergence_position}</span>
                </div>
              </div>
            )}
            {divergence.backend_changed && (
              <div className="flex items-start gap-2">
                <AlertCircle className="size-4 text-yellow-500 mt-0.5" />
                <div>
                  <span className="font-medium">Backend Changed:</span>
                  <span className="ml-2">Different backend used for replay</span>
                </div>
              </div>
            )}
            {divergence.manifest_changed && (
              <div className="flex items-start gap-2">
                <AlertCircle className="size-4 text-yellow-500 mt-0.5" />
                <div>
                  <span className="font-medium">Manifest Changed:</span>
                  <span className="ml-2">Model manifest differs from original</span>
                </div>
              </div>
            )}
            {divergence.approximation_reasons.length > 0 && (
              <div className="mt-3 pt-3 border-t border-border">
                <span className="font-medium block mb-2">Approximation Reasons:</span>
                <ul className="list-disc list-inside space-y-1 text-muted-foreground">
                  {divergence.approximation_reasons.map((reason, idx) => (
                    <li key={idx}>{reason}</li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Truncation Warning */}
      {replayResponse.response_truncated && (
        <div className="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-900 rounded-md p-3">
          <div className="flex items-start gap-2">
            <AlertTriangle className="size-4 text-yellow-600 dark:text-yellow-500 mt-0.5" />
            <div className="text-sm">
              <span className="font-medium">Response Truncated:</span>
              <span className="ml-2 text-muted-foreground">
                Replay response was truncated to 64KB storage limit
              </span>
            </div>
          </div>
        </div>
      )}

      {/* Replay IDs */}
      <div className="space-y-2">
        <h4 className="text-sm font-medium">Replay Information</h4>
        <div className="bg-muted/50 rounded-md p-3 space-y-2 text-sm font-mono">
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Replay ID:</span>
            <span className="text-foreground">{replayResponse.replay_id}</span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Original Inference:</span>
            <span className="text-foreground">{replayResponse.original_inference_id}</span>
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * Statistics Tab Component
 * Shows token counts, latency comparison, and RAG reproducibility
 */
function StatisticsTab({ replayResponse }: { replayResponse: ReplayResponse }) {
  const { stats, rag_reproducibility } = replayResponse;

  return (
    <div className="space-y-4">
      {/* Performance Metrics */}
      <div className="space-y-2">
        <h4 className="text-sm font-medium">Performance Metrics</h4>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <div className="bg-muted/50 rounded-md p-3">
            <div className="flex items-center gap-2 mb-2">
              <Zap className="size-4 text-blue-500" />
              <span className="text-xs font-medium text-muted-foreground">Tokens Generated</span>
            </div>
            <div className="text-2xl font-semibold">{stats.tokens_generated}</div>
          </div>
          <div className="bg-muted/50 rounded-md p-3">
            <div className="flex items-center gap-2 mb-2">
              <Clock className="size-4 text-purple-500" />
              <span className="text-xs font-medium text-muted-foreground">Latency</span>
            </div>
            <div className="text-2xl font-semibold">{stats.latency_ms}ms</div>
            {stats.original_latency_ms !== undefined && (
              <div className="text-xs text-muted-foreground mt-1">
                Original: {stats.original_latency_ms}ms
                {stats.latency_ms !== stats.original_latency_ms && (
                  <span className={stats.latency_ms < stats.original_latency_ms ? 'text-green-600' : 'text-red-600'}>
                    {' '}({stats.latency_ms > stats.original_latency_ms ? '+' : ''}
                    {stats.latency_ms - stats.original_latency_ms}ms)
                  </span>
                )}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Latency Comparison Chart (if original available) */}
      {stats.original_latency_ms !== undefined && (
        <div className="space-y-2">
          <h4 className="text-sm font-medium">Latency Comparison</h4>
          <div className="bg-muted/50 rounded-md p-3">
            <div className="space-y-3">
              <div>
                <div className="flex items-center justify-between mb-1">
                  <span className="text-xs text-muted-foreground">Original</span>
                  <span className="text-xs font-medium">{stats.original_latency_ms}ms</span>
                </div>
                <Progress
                  value={(stats.original_latency_ms / Math.max(stats.latency_ms, stats.original_latency_ms)) * 100}
                  className="h-2"
                  aria-label="Original latency"
                />
              </div>
              <div>
                <div className="flex items-center justify-between mb-1">
                  <span className="text-xs text-muted-foreground">Replay</span>
                  <span className="text-xs font-medium">{stats.latency_ms}ms</span>
                </div>
                <Progress
                  value={(stats.latency_ms / Math.max(stats.latency_ms, stats.original_latency_ms)) * 100}
                  className="h-2"
                  aria-label="Replay latency"
                />
              </div>
            </div>
          </div>
        </div>
      )}

      {/* RAG Reproducibility */}
      {rag_reproducibility && (
        <div className="space-y-2">
          <h4 className="text-sm font-medium">RAG Reproducibility</h4>
          <div className="bg-muted/50 rounded-md p-3 space-y-3">
            <div>
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <Database className="size-4 text-indigo-500" />
                  <span className="text-sm font-medium">Document Availability</span>
                </div>
                <span className="text-sm font-semibold">
                  {getRagReproducibilityPercent(rag_reproducibility)}%
                </span>
              </div>
              <Progress
                value={rag_reproducibility.score * 100}
                className="h-2"
                aria-label="RAG reproducibility score"
              />
              <div className="flex items-center justify-between mt-2 text-xs text-muted-foreground">
                <span>
                  {rag_reproducibility.matching_docs} / {rag_reproducibility.total_original_docs} documents
                </span>
                {rag_reproducibility.missing_doc_ids.length > 0 && (
                  <span className="text-orange-600 dark:text-orange-400">
                    {rag_reproducibility.missing_doc_ids.length} missing
                  </span>
                )}
              </div>
            </div>
            {rag_reproducibility.missing_doc_ids.length > 0 && (
              <div className="pt-3 border-t border-border">
                <span className="text-xs font-medium block mb-2">Missing Documents:</span>
                <div className="space-y-1 max-h-[120px] overflow-y-auto">
                  {rag_reproducibility.missing_doc_ids.map((docId) => (
                    <div key={docId} className="text-xs font-mono text-muted-foreground bg-background rounded px-2 py-1">
                      {docId}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Main ReplayResultDialog Component
 */
export function ReplayResultDialog({
  open,
  onOpenChange,
  replayResponse,
  onViewHistory,
}: ReplayResultDialogProps) {
  const [activeTab, setActiveTab] = useState<string>('comparison');

  // Reset to comparison tab when dialog opens
  React.useEffect(() => {
    if (open) {
      setActiveTab('comparison');
    }
  }, [open]);

  if (!replayResponse) {
    return null;
  }

  const badge = getMatchStatusBadge(replayResponse.match_status);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <div className="flex items-center justify-between">
            <DialogTitle className="flex items-center gap-2">
              Replay Results
              <Badge variant={badge.variant}>
                {badge.icon}
                {getMatchStatusLabel(replayResponse.match_status)}
              </Badge>
            </DialogTitle>
            {onViewHistory && (
              <Button variant="ghost" size="sm" onClick={onViewHistory}>
                <History className="size-4 mr-2" />
                View History
              </Button>
            )}
          </div>
        </DialogHeader>

        <Tabs value={activeTab} onValueChange={setActiveTab} className="mt-4">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="comparison">Comparison</TabsTrigger>
            <TabsTrigger value="details">Details</TabsTrigger>
            <TabsTrigger value="statistics">Statistics</TabsTrigger>
          </TabsList>

          <TabsContent value="comparison" className="mt-4">
            <ResponseComparison
              original={replayResponse.original_response}
              replay={replayResponse.response}
              divergencePosition={replayResponse.divergence?.divergence_position}
            />
          </TabsContent>

          <TabsContent value="details" className="mt-4">
            <DetailsTab replayResponse={replayResponse} />
          </TabsContent>

          <TabsContent value="statistics" className="mt-4">
            <StatisticsTab replayResponse={replayResponse} />
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}

export default ReplayResultDialog;
