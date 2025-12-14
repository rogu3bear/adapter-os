/**
 * Replay Result Dialog Component
 *
 * Displays replay execution results with comparison, details, and statistics.
 * Part of Deterministic Replay feature.
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
  Scale,
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
  getRagReproducibilityPercent,
  formatLatencyDiff,
} from '@/api/replay-types';
import { ProofBar } from '@/components/receipts/ProofBar';
import { useNavigate } from 'react-router-dom';

interface ReplayResultDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  replayResponse: ReplayResponse | null;
  onViewHistory?: () => void;
}

/**
 * Human-facing verification labels for match status
 * Maps engineering terms to user-friendly labels:
 * - Verified = exact match
 * - Balanced = semantic match
 * - Unverified = divergent
 */
const VERIFICATION_LABELS: Record<ReplayMatchStatus, {
  human: string;
  engineering: string;
}> = {
  exact: { human: 'Verified', engineering: 'Exact' },
  semantic: { human: 'Balanced', engineering: 'Semantic' },
  divergent: { human: 'Unverified', engineering: 'Divergent' },
  error: { human: 'Error', engineering: 'Error' },
};

/**
 * Get badge variant and color for match status
 */
function getMatchStatusBadge(status: ReplayMatchStatus): {
  variant: 'success' | 'warning' | 'error' | 'neutral';
  icon: React.ReactNode;
  humanLabel: string;
  engineeringLabel: string;
} {
  const labels = VERIFICATION_LABELS[status] ?? { human: 'Unknown', engineering: 'Unknown' };

  switch (status) {
    case 'exact':
      return {
        variant: 'success',
        icon: <CheckCircle2 className="size-4" />,
        humanLabel: labels.human,
        engineeringLabel: labels.engineering,
      };
    case 'semantic':
      return {
        variant: 'warning',
        icon: <Scale className="size-4" />,
        humanLabel: labels.human,
        engineeringLabel: labels.engineering,
      };
    case 'divergent':
      return {
        variant: 'error',
        icon: <AlertTriangle className="size-4" />,
        humanLabel: labels.human,
        engineeringLabel: labels.engineering,
      };
    case 'error':
      return {
        variant: 'error',
        icon: <XCircle className="size-4" />,
        humanLabel: labels.human,
        engineeringLabel: labels.engineering,
      };
    default:
      return {
        variant: 'neutral',
        icon: <AlertCircle className="size-4" />,
        humanLabel: 'Unknown',
        engineeringLabel: 'Unknown',
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
            <div className="bg-muted/50 rounded-md p-3 text-sm font-mono whitespace-pre-wrap break-words max-h-[calc(var(--base-unit)*100)] overflow-y-auto">
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
            <div className="bg-muted/50 rounded-md p-3 text-sm font-mono whitespace-pre-wrap break-words max-h-[calc(var(--base-unit)*100)] overflow-y-auto">
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
      <div className="bg-muted/50 rounded-md p-3 text-sm font-mono whitespace-pre-wrap break-words max-h-[calc(var(--base-unit)*100)] overflow-y-auto">
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
          <div className="flex flex-col items-start gap-0.5">
            <Badge variant={badge.variant} className="gap-1.5">
              {badge.icon}
              {badge.humanLabel}
            </Badge>
            <span className="text-xs text-muted-foreground">
              ({badge.engineeringLabel})
            </span>
          </div>
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

      {/* Version Consistency Warning */}
      {replayResponse.version_consistency_warning && (
        <div className="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-900 rounded-md p-3">
          <div className="flex items-start gap-2">
            <AlertTriangle className="size-4 text-yellow-600 dark:text-yellow-500 mt-0.5" />
            <div className="text-sm">
              <span className="font-medium">Version Mismatch:</span>
              <span className="ml-2 text-muted-foreground">
                {replayResponse.version_consistency_warning}
              </span>
            </div>
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

      {/* Version Pins */}
      {(replayResponse.replay_key?.dataset_version_id ||
        replayResponse.replay_key?.rag_snapshot_hash ||
        (replayResponse.replay_key?.adapter_ids && replayResponse.replay_key.adapter_ids.length > 0)) && (
        <div className="space-y-2">
          <h4 className="text-sm font-medium flex items-center gap-2">
            <Database className="h-4 w-4" />
            Version Pins
          </h4>
          <div className="bg-muted/50 rounded-md p-3 space-y-2 text-sm">
            {replayResponse.replay_key?.dataset_version_id && (
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">Dataset Version:</span>
                <code className="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">
                  {replayResponse.replay_key.dataset_version_id}
                </code>
              </div>
            )}
            {replayResponse.replay_key?.rag_snapshot_hash && (
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">RAG Snapshot:</span>
                <code className="font-mono text-xs bg-muted px-1.5 py-0.5 rounded truncate max-w-[200px]" title={replayResponse.replay_key.rag_snapshot_hash}>
                  {replayResponse.replay_key.rag_snapshot_hash.substring(0, 16)}...
                </code>
              </div>
            )}
            {replayResponse.replay_key?.adapter_ids && replayResponse.replay_key.adapter_ids.length > 0 && (
              <div className="flex items-start justify-between">
                <span className="text-muted-foreground">Adapters:</span>
                <div className="flex flex-wrap gap-1 justify-end">
                  {replayResponse.replay_key.adapter_ids.map((id, i) => (
                    <code key={i} className="font-mono text-xs bg-muted px-1.5 py-0.5 rounded">
                      {id.substring(0, 8)}
                    </code>
                  ))}
                </div>
              </div>
            )}
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

      {/* Citation Stability */}
      {rag_reproducibility && (
        <div className="space-y-2">
          <h4 className="text-sm font-medium">Citation Stability</h4>
          <div className="bg-muted/50 rounded-md p-3 space-y-3">
            {/* Stability summary */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Database className="size-4 text-indigo-500" />
                <span className="text-sm">
                  {rag_reproducibility.matching_docs} of {rag_reproducibility.total_original_docs} citations unchanged
                </span>
              </div>
              <Badge
                variant={rag_reproducibility.score === 1 ? 'success' : rag_reproducibility.score >= 0.8 ? 'warning' : 'error'}
                className="text-xs"
              >
                {getRagReproducibilityPercent(rag_reproducibility)}% stable
              </Badge>
            </div>

            {/* Progress bar */}
            <Progress
              value={rag_reproducibility.score * 100}
              className="h-2"
              aria-label="Citation stability score"
            />

            {/* Changed citations list */}
            {rag_reproducibility.missing_doc_ids.length > 0 && (
              <div className="pt-3 border-t border-border">
                <span className="text-xs font-medium block mb-2 text-orange-600 dark:text-orange-400">
                  Changed Citations ({rag_reproducibility.missing_doc_ids.length}):
                </span>
                <div className="space-y-1 max-h-[120px] overflow-y-auto">
                  {rag_reproducibility.missing_doc_ids.map((docId) => (
                    <div key={docId} className="text-xs font-mono text-muted-foreground bg-background rounded px-2 py-1">
                      {docId}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* All citations stable */}
            {rag_reproducibility.missing_doc_ids.length === 0 && rag_reproducibility.total_original_docs > 0 && (
              <div className="flex items-center gap-2 text-sm text-green-600 dark:text-green-400">
                <CheckCircle2 className="size-4" />
                All citations reproduced exactly
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
  const navigate = useNavigate();

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
  const handleOpenTrace = () => {
    const traceId = replayResponse.original_inference_id;
    if (!traceId) {
      toast.error('Trace ID is unavailable');
      return;
    }
    navigate(`/telemetry?tab=viewer&requestId=${encodeURIComponent(traceId)}`);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <div className="flex items-center justify-between">
            <DialogTitle className="flex items-center gap-2">
              Replay Results
              <div className="flex items-center gap-2">
                <div className="flex flex-col items-start gap-0.5">
                  <Badge variant={badge.variant} className="text-sm px-2.5 py-0.5 gap-1.5">
                    {badge.icon}
                    {badge.humanLabel}
                  </Badge>
                  <span className="text-xs text-muted-foreground ml-1">
                    ({badge.engineeringLabel})
                  </span>
                </div>
                {replayResponse.version_consistency_warning && (
                  <Badge variant="warning" className="gap-1.5 bg-yellow-100 text-yellow-800 dark:bg-yellow-900/40 dark:text-yellow-200">
                    <AlertTriangle className="h-3 w-3" />
                    Version Mismatch
                  </Badge>
                )}
              </div>
            </DialogTitle>
            {onViewHistory && (
              <Button variant="ghost" size="sm" onClick={onViewHistory}>
                <History className="size-4 mr-2" />
                View History
              </Button>
            )}
          </div>
        </DialogHeader>
        <ProofBar
          traceId={replayResponse.original_inference_id}
          receiptDigest={undefined}
          backendUsed={undefined}
          determinismMode={undefined}
          evidenceAvailable={false}
          onOpenTrace={handleOpenTrace}
          className="mt-2"
        />

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
