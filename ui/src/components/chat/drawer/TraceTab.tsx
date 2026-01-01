/**
 * TraceTab - Displays trace information for a chat message
 *
 * Shows trace summary (digests, backend, kernel) and a condensed token table.
 * Part of the EvidenceDrawer tabs alongside Rulebook and Calculation.
 */

import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Activity, Copy, ExternalLink, Loader2, AlertCircle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Skeleton } from '@/components/ui/skeleton';
import { toast } from 'sonner';
import { useTrace } from '@/hooks/observability/useTrace';
import { useTenant } from '@/providers/FeatureProviders';
import { buildTelemetryTraceLink } from '@/utils/navLinks';

interface TraceTabProps {
  /** Trace ID to fetch and display */
  traceId?: string | null;
  /** Callback when user clicks "Open full viewer" */
  onOpenFullViewer?: () => void;
}

/** Maximum number of tokens to show inline before "show more" link */
const MAX_INLINE_TOKENS = 10;

function copyToClipboard(text: string, label: string) {
  navigator.clipboard.writeText(text).then(
    () => toast.success(`${label} copied`),
    () => toast.error(`Failed to copy ${label}`)
  );
}

function TruncatedDigest({
  label,
  value,
}: {
  label: string;
  value: string;
}) {
  const truncated = value.length > 20
    ? `${value.slice(0, 10)}...${value.slice(-10)}`
    : value;

  return (
    <div className="flex items-center justify-between gap-2 py-2 border-b last:border-b-0">
      <span className="text-xs text-muted-foreground">{label}</span>
      <div className="flex items-center gap-2">
        <span className="font-mono text-xs" title={value}>
          {truncated}
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          onClick={() => copyToClipboard(value, label)}
          aria-label={`Copy ${label}`}
        >
          <Copy className="h-3 w-3" />
        </Button>
      </div>
    </div>
  );
}

function LoadingSkeleton() {
  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <Skeleton className="h-4 w-32" />
        </CardHeader>
        <CardContent className="space-y-3">
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-full" />
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <Skeleton className="h-4 w-40" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-32 w-full" />
        </CardContent>
      </Card>
    </div>
  );
}

export function TraceTab({ traceId, onOpenFullViewer }: TraceTabProps) {
  const navigate = useNavigate();
  const { selectedTenant } = useTenant();
  const { data: trace, isLoading, isError, error } = useTrace(
    traceId ?? undefined,
    selectedTenant ?? undefined
  );
  const handleOpenFullViewer = () => {
    if (onOpenFullViewer) {
      onOpenFullViewer();
    } else if (traceId) {
      navigate(buildTelemetryTraceLink(traceId));
    }
  };

  // Get first N tokens for condensed view
  const displayTokens = useMemo(() => {
    if (!trace?.tokens) return [];
    return trace.tokens.slice(0, MAX_INLINE_TOKENS);
  }, [trace]);

  const hasMoreTokens = trace?.tokens && trace.tokens.length > MAX_INLINE_TOKENS;

  // Empty state - no trace ID provided
  if (!traceId) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-center">
        <Activity className="h-12 w-12 text-muted-foreground/50 mb-4" />
        <p className="text-sm text-muted-foreground">No trace available</p>
        <p className="text-xs text-muted-foreground mt-1">
          Select a message with trace data to view details
        </p>
      </div>
    );
  }

  // Loading state
  if (isLoading) {
    return <LoadingSkeleton />;
  }

  // Error state
  if (isError) {
    return (
      <Alert variant="destructive">
        <AlertCircle className="h-4 w-4" />
        <AlertDescription>
          Failed to load trace: {error instanceof Error ? error.message : 'Unknown error'}
        </AlertDescription>
      </Alert>
    );
  }

  // No trace data returned
  if (!trace) {
    return (
      <Alert>
        <AlertCircle className="h-4 w-4" />
        <AlertDescription>
          Trace data not found for ID: {traceId}
        </AlertDescription>
      </Alert>
    );
  }

  return (
    <ScrollArea className="h-full">
      <div className="space-y-4 pr-4">
        {/* Trace Summary */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Trace Summary</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1">
            <TruncatedDigest label="Trace ID" value={trace.trace_id} />
            <TruncatedDigest label="Context digest" value={trace.context_digest} />
            <TruncatedDigest label="Policy digest" value={trace.policy_digest} />

            {/* Metadata badges */}
            <div className="flex flex-wrap gap-2 pt-3 mt-2 border-t">
              <Badge variant="outline" className="text-xs">
                Backend: {trace.backend_id}
              </Badge>
              <Badge variant="outline" className="text-xs">
                Kernel: {trace.kernel_version_id}
              </Badge>
              <Badge variant="secondary" className="text-xs">
                {trace.tokens.length} tokens
              </Badge>
              <Badge variant="outline" className="text-xs">
                Dense routing
              </Badge>
            </div>
          </CardContent>
        </Card>

        {/* Token Decisions (condensed) */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between">
            <CardTitle className="text-sm">Token Decisions</CardTitle>
            {hasMoreTokens && (
              <Badge variant="secondary" className="text-xs">
                Showing {MAX_INLINE_TOKENS} of {trace.tokens.length}
              </Badge>
            )}
          </CardHeader>
          <CardContent>
            {displayTokens.length === 0 ? (
              <p className="text-sm text-muted-foreground">No token data available</p>
            ) : (
              <div className="space-y-2">
                {/* Table header */}
                <div className="grid grid-cols-[40px_1fr] gap-2 text-xs font-medium text-muted-foreground pb-2 border-b">
                  <span>#</span>
                  <span>Adapter : Gate (Q15)</span>
                </div>

                {/* Token rows */}
                {displayTokens.map((token) => (
                  <div
                    key={token.token_index}
                    className="grid grid-cols-[40px_1fr] gap-2 text-xs items-start py-1"
                  >
                    <span className="font-mono text-muted-foreground">
                      {token.token_index}
                    </span>
                    <div className="flex flex-wrap gap-1">
                      {token.selected_adapter_ids.length > 0 ? (
                        token.selected_adapter_ids.map((adapterId, idx) => {
                          const gateValue = token.gates_q15[idx];
                          const gateDisplay = gateValue !== undefined
                            ? gateValue.toLocaleString()
                            : '?';
                          return (
                            <Badge
                              key={`${adapterId}-${idx}`}
                              variant="secondary"
                              className="text-xs font-mono"
                              title={`${adapterId}: ${gateDisplay}`}
                            >
                              {adapterId.length > 12
                                ? `${adapterId.slice(0, 6)}...${adapterId.slice(-4)}`
                                : adapterId}
                              <span className="text-muted-foreground ml-1">:</span>
                              <span className="ml-1">{gateDisplay}</span>
                            </Badge>
                          );
                        })
                      ) : (
                        <span className="text-muted-foreground italic">no adapters</span>
                      )}
                    </div>
                  </div>
                ))}

                {/* Show more link */}
                {hasMoreTokens && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="w-full mt-2 text-xs"
                    onClick={handleOpenFullViewer}
                  >
                    Show all {trace.tokens.length} tokens
                    <ExternalLink className="h-3 w-3 ml-2" />
                  </Button>
                )}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Actions */}
        <Card>
          <CardContent className="pt-4">
            <Button
              variant="outline"
              size="sm"
              onClick={handleOpenFullViewer}
              className="w-full gap-2"
            >
              <ExternalLink className="h-4 w-4" />
              Open full trace viewer
            </Button>
          </CardContent>
        </Card>
      </div>
    </ScrollArea>
  );
}

export default TraceTab;
