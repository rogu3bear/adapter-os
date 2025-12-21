import React from 'react';
import { useNavigate } from 'react-router-dom';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Copy, ExternalLink, CheckCircle2, Clock, Zap } from 'lucide-react';
import { toast } from 'sonner';
import type { ExtendedRouterDecision } from '@/api/types';
import { buildTelemetryTraceLink } from '@/utils/navLinks';

interface CalculationTabProps {
  requestId?: string | null;
  routerDecision?: ExtendedRouterDecision | null;
  traceId?: string | null;
  proofDigest?: string | null | undefined;
  isVerified?: boolean;
  verifiedAt?: string | null;
  throughputStats?: { tokensGenerated: number; latencyMs: number; tokensPerSecond: number } | null;
}

function CopyButton({ value, label }: { value: string; label: string }) {
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      toast.success(`${label} copied`);
    } catch {
      toast.error(`Failed to copy ${label}`);
    }
  };

  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={handleCopy}
      className="h-8 w-8 text-muted-foreground hover:text-foreground"
      aria-label={`Copy ${label}`}
    >
      <Copy className="h-4 w-4" />
    </Button>
  );
}

function TruncatedText({
  text,
  maxLength = 16,
}: {
  text: string;
  maxLength?: number;
}) {
  if (text.length <= maxLength) {
    return <span className="font-mono text-sm">{text}</span>;
  }

  const start = text.slice(0, Math.floor(maxLength / 2));
  const end = text.slice(-Math.floor(maxLength / 2));

  return (
    <span className="font-mono text-sm" title={text}>
      {start}...{end}
    </span>
  );
}

export function CalculationTab({
  requestId,
  routerDecision,
  traceId,
  proofDigest,
  isVerified,
  verifiedAt,
  throughputStats,
}: CalculationTabProps) {
  const navigate = useNavigate();

  const handleOpenTrace = () => {
    if (traceId) {
      navigate(buildTelemetryTraceLink(traceId));
    }
  };

  // Calculate gate weights as percentages if available
  const gatePercentages = React.useMemo(() => {
    if (!routerDecision?.candidates) return null;

    const selectedCandidates = routerDecision.candidates.filter((c) => c.selected);
    if (selectedCandidates.length === 0) return null;

    const total = selectedCandidates.reduce((sum, c) => sum + c.gate_float, 0);
    if (total === 0) return null;

    return selectedCandidates.map((c) => ({
      adapter_id: c.adapter_id,
      percentage: (c.gate_float / total) * 100,
      gate_value: c.gate_float,
    }));
  }, [routerDecision]);

  return (
    <ScrollArea className="h-full">
      <div className="space-y-4 pr-4">
        {/* Proof Summary */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Proof Summary</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            {/* Trace ID */}
            {traceId && (
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs text-muted-foreground">Trace ID</span>
                <div className="flex items-center gap-2">
                  <TruncatedText text={traceId} maxLength={20} />
                  <CopyButton value={traceId} label="Trace ID" />
                </div>
              </div>
            )}

            {/* Proof Digest */}
            {proofDigest && (
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs text-muted-foreground">Proof Digest</span>
                <div className="flex items-center gap-2">
                  <TruncatedText text={proofDigest} maxLength={20} />
                  <CopyButton value={proofDigest} label="Proof Digest" />
                </div>
              </div>
            )}

            {/* Verification Status */}
            {isVerified !== undefined && (
              <div className="flex items-center justify-between gap-2 pt-2 border-t">
                <span className="text-xs text-muted-foreground">Verification</span>
                <div className="flex items-center gap-2">
                  {isVerified ? (
                    <>
                      <CheckCircle2 className="h-4 w-4 text-green-600" />
                      <Badge variant="success" className="text-xs">
                        Verified
                      </Badge>
                    </>
                  ) : (
                    <>
                      <Clock className="h-4 w-4 text-yellow-600" />
                      <Badge variant="warning" className="text-xs">
                        Pending
                      </Badge>
                    </>
                  )}
                </div>
              </div>
            )}

            {/* Verified At */}
            {verifiedAt && (
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs text-muted-foreground">Verified At</span>
                <span className="text-xs font-mono">
                  {new Date(verifiedAt).toLocaleString()}
                </span>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Performance Metrics */}
        {throughputStats && (
          <Card>
            <CardHeader>
              <CardTitle className="text-sm flex items-center gap-2">
                <Zap className="h-4 w-4" />
                Performance
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-3 gap-2">
                <div className="bg-muted/50 rounded-md p-3 text-center">
                  <div className="text-xl font-semibold">
                    {throughputStats.tokensGenerated}
                  </div>
                  <div className="text-xs text-muted-foreground mt-1">Tokens</div>
                </div>
                <div className="bg-muted/50 rounded-md p-3 text-center">
                  <div className="text-xl font-semibold">
                    {(throughputStats.latencyMs / 1000).toFixed(1)}s
                  </div>
                  <div className="text-xs text-muted-foreground mt-1">Time</div>
                </div>
                <div className="bg-muted/50 rounded-md p-3 text-center">
                  <div className="text-xl font-semibold">
                    {throughputStats.tokensPerSecond.toFixed(1)}
                  </div>
                  <div className="text-xs text-muted-foreground mt-1">tok/s</div>
                </div>
              </div>
            </CardContent>
          </Card>
        )}

        {/* Routing Decision */}
        {routerDecision && (
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Routing Decision</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              {/* Selected Adapters */}
              {routerDecision.selected_adapters &&
                routerDecision.selected_adapters.length > 0 && (
                  <div>
                    <span className="text-xs text-muted-foreground mb-2 block">
                      Selected Adapters
                    </span>
                    <div className="flex flex-wrap gap-2">
                      {routerDecision.selected_adapters.map((adapterId) => (
                        <Badge key={adapterId} variant="secondary" className="text-xs">
                          {adapterId}
                        </Badge>
                      ))}
                    </div>
                  </div>
                )}

              {/* Gate Weights */}
              {gatePercentages && (
                <div>
                  <span className="text-xs text-muted-foreground mb-2 block">
                    Gate Weights
                  </span>
                  <div className="space-y-2">
                    {gatePercentages.map((item) => (
                      <div
                        key={item.adapter_id}
                        className="flex items-center justify-between gap-2"
                      >
                        <span className="text-xs font-mono truncate flex-1">
                          {item.adapter_id}
                        </span>
                        <div className="flex items-center gap-2">
                          <div className="w-24 h-2 bg-muted rounded-full overflow-hidden">
                            <div
                              className="h-full bg-primary"
                              style={{ width: `${item.percentage}%` }}
                            />
                          </div>
                          <span className="text-xs font-medium w-12 text-right">
                            {item.percentage.toFixed(1)}%
                          </span>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Router Parameters */}
              <div className="pt-2 border-t space-y-2">
                {routerDecision.k_value !== undefined && (
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs text-muted-foreground">K-value</span>
                    <span className="text-xs font-mono">
                      {routerDecision.k_value}
                    </span>
                  </div>
                )}

                {routerDecision.entropy !== undefined && (
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs text-muted-foreground">Entropy</span>
                    <span className="text-xs font-mono">
                      {routerDecision.entropy.toFixed(4)}
                    </span>
                  </div>
                )}

                {routerDecision.tau !== undefined && (
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs text-muted-foreground">Tau</span>
                    <span className="text-xs font-mono">
                      {routerDecision.tau.toFixed(4)}
                    </span>
                  </div>
                )}

                {routerDecision.latency_ms !== undefined && (
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs text-muted-foreground">Latency</span>
                    <span className="text-xs font-mono">
                      {routerDecision.latency_ms.toFixed(2)}ms
                    </span>
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        )}

        {/* Actions */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Actions</CardTitle>
          </CardHeader>
          <CardContent>
            <Button
              variant="outline"
              size="sm"
              onClick={handleOpenTrace}
              disabled={!traceId}
              className="w-full gap-2"
            >
              <ExternalLink className="h-4 w-4" />
              Open Trace in Telemetry Viewer
            </Button>
          </CardContent>
        </Card>

        {/* Empty state when no data */}
        {!traceId && !proofDigest && !routerDecision && !throughputStats && (
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <p className="text-sm text-muted-foreground">No calculation data available</p>
            <p className="text-xs text-muted-foreground mt-1">
              Inference metadata will appear here
            </p>
          </div>
        )}
      </div>
    </ScrollArea>
  );
}
