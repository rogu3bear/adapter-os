import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip';
import {
  Activity,
  Target,
  FileText,
  Zap,
  Clock,
  Info,
  HelpCircle,
} from 'lucide-react';
import { InferenceTrace } from '@/api/types';

interface TraceVisualizerProps {
  trace: InferenceTrace;
}

export function TraceVisualizer({ trace }: TraceVisualizerProps) {
  const traceModelType = trace.model_type || trace.router_decisions?.[0]?.model_type;
  const moeInfo = trace.moe_info;

  return (
    <Card>
      <CardHeader className="pb-3 flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <CardTitle className="text-base flex items-center gap-2">
            <Activity className="h-4 w-4" />
            Inference Trace
          </CardTitle>
          {traceModelType && (
            <Badge
              variant={traceModelType === 'moe' ? 'secondary' : 'outline'}
              className="text-xs"
            >
              {traceModelType === 'moe' ? 'MoE (ghost experts)' : 'Dense routing'}
            </Badge>
          )}
        </div>
        {moeInfo && (
          <div className="text-xs text-muted-foreground">
            {moeInfo.num_experts} experts · {moeInfo.experts_per_token} per token
          </div>
        )}
      </CardHeader>
      <CardContent>
        {/* Trace Intro Section */}
        <Alert className="mb-4">
          <Info className="h-4 w-4" />
          <AlertDescription>
            Trace shows the internal reasoning: which adapters were selected (Router), what documents supported the answer (Evidence), and performance metrics.
          </AlertDescription>
        </Alert>

        <Tabs defaultValue="router" className="space-y-4">
          <TabsList>
            <TabsTrigger value="router">
              <Target className="h-4 w-4 mr-2" />
              Router
            </TabsTrigger>
            <TabsTrigger value="evidence">
              <FileText className="h-4 w-4 mr-2" />
              Evidence
            </TabsTrigger>
            <TabsTrigger value="performance">
              <Zap className="h-4 w-4 mr-2" />
              Performance
            </TabsTrigger>
          </TabsList>

          {/* Router Decisions */}
          <TabsContent value="router" className="space-y-3">
            {trace.router_decisions && trace.router_decisions.length > 0 ? (
              <>
                <div className="text-sm text-muted-foreground">
                  {trace.router_decisions.length} routing decisions
                </div>
                <div className="space-y-2 max-h-[300px] overflow-y-auto">
                  {trace.router_decisions.slice(0, 10).map((decision, idx) => (
                    <div key={idx} className="p-3 bg-muted rounded-lg text-sm">
                      {(() => {
                        const decisionModelType =
                          decision.model_type || trace.model_type || 'dense';
                        const activeExperts =
                          decision.active_experts || trace.active_experts?.[idx];
                        const isMoe = decisionModelType === 'moe';
                        return (
                          <>
                            <div className="flex items-center justify-between mb-1">
                              <div className="flex flex-col">
                                <span className="font-medium">
                                  Token {decision.step || decision.token_idx}
                                  {decision.input_token_id !== undefined
                                    ? ` (input ${decision.input_token_id})`
                                    : ''}
                                </span>
                                {decision.entropy !== undefined && (
                                  <span className="text-xs text-muted-foreground flex items-center gap-1">
                                    Entropy: {decision.entropy.toFixed(3)}, Tau:{' '}
                                    {decision.tau?.toFixed(3) || 'N/A'}, Floor:{' '}
                                    {decision.entropy_floor?.toFixed(3) || 'N/A'}
                                    <Tooltip>
                                      <TooltipTrigger asChild>
                                        <HelpCircle className="h-3 w-3 cursor-help text-muted-foreground/60" />
                                      </TooltipTrigger>
                                      <TooltipContent side="right" className="max-w-xs">
                                        <p><strong>Entropy:</strong> Uncertainty in token prediction. Higher values trigger more adapter routing.</p>
                                        <p className="mt-1"><strong>Tau:</strong> Temperature scaling factor for routing decisions.</p>
                                        <p className="mt-1"><strong>Floor:</strong> Minimum entropy threshold before routing activates.</p>
                                      </TooltipContent>
                                    </Tooltip>
                                  </span>
                                )}
                              </div>
                              <Badge variant="outline">
                                {decision.candidate_adapters?.length || decision.adapters?.length || 0} adapters
                              </Badge>
                            </div>
                            {isMoe && (
                              <div className="mb-2 flex flex-wrap items-center gap-2">
                                <Badge variant="secondary" className="text-[10px] uppercase">
                                  Ghost experts
                                </Badge>
                                {activeExperts && activeExperts.length ? (
                                  activeExperts.map((expert) => (
                                    <Badge
                                      key={expert}
                                      variant="outline"
                                      className="text-[11px]"
                                    >
                                      Expert {expert}
                                    </Badge>
                                  ))
                                ) : (
                                  <span className="text-xs text-muted-foreground">
                                    No expert trace captured
                                  </span>
                                )}
                              </div>
                            )}
                          </>
                        );
                      })()}

                      <div className="space-y-1">
                        <div className="flex items-center gap-2 text-xs font-semibold text-muted-foreground">
                          <span>User adapters</span>
                        </div>
                        {decision.candidate_adapters?.map((candidate, candidateIdx) => {
                          if (typeof candidate === 'string') return null;
                          return (
                            <div
                              key={candidateIdx}
                              className="flex items-center justify-between text-xs"
                            >
                              <span className="font-mono">
                                Adapter {candidate.adapter_idx}
                              </span>
                              <span className="text-muted-foreground">
                                Score: {candidate.raw_score.toFixed(3)} | Gate:{' '}
                                {candidate.gate_q15}
                              </span>
                            </div>
                          );
                        }) || decision.adapters?.map((adapterId, adapterIdx) => (
                          <div
                            key={adapterIdx}
                            className="flex items-center justify-between text-xs"
                          >
                            <span className="font-mono">{adapterId}</span>
                            <span className="text-muted-foreground">
                              Gate: {decision.gates?.[adapterIdx] || 0}
                            </span>
                          </div>
                        ))}
                      </div>

                      {decision.stack_hash && (
                        <div className="text-xs text-muted-foreground mt-2">
                          Stack hash: {decision.stack_hash.slice(0, 12)}...
                        </div>
                      )}
                    </div>
                  ))}
                </div>
                {trace.router_decisions.length > 10 && (
                  <div className="text-sm text-muted-foreground text-center">
                    + {trace.router_decisions.length - 10} more decisions
                  </div>
                )}
              </>
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                <Target className="h-8 w-8 mx-auto mb-2 opacity-20" />
                <p>No routing decisions available</p>
                {traceModelType === 'moe' && (
                  <p className="text-xs text-muted-foreground mt-1">
                    MoE trace detected; ghost experts recorded for {trace.active_experts?.length ?? 0} tokens.
                  </p>
                )}
              </div>
            )}
          </TabsContent>

          {/* Evidence Spans */}
          <TabsContent value="evidence" className="space-y-3">
            {trace.evidence_spans && trace.evidence_spans.length > 0 ? (
              <>
                <div className="text-sm text-muted-foreground flex items-center gap-1">
                  {trace.evidence_spans.length} evidence spans
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <HelpCircle className="h-3 w-3 cursor-help text-muted-foreground/60" />
                    </TooltipTrigger>
                    <TooltipContent side="right" className="max-w-xs">
                      <p>Evidence spans are retrieved documents from RAG (Retrieval-Augmented Generation) that support the model's response. Each span shows the source document and relevant text passage used during inference.</p>
                    </TooltipContent>
                  </Tooltip>
                </div>
                <div className="space-y-2 max-h-[300px] overflow-y-auto">
                  {trace.evidence_spans.map((span, idx) => (
                    <div
                      key={idx}
                      className="p-3 bg-muted rounded-lg text-sm"
                    >
                      <div className="flex items-center gap-2 mb-2">
                        <FileText className="h-4 w-4 text-muted-foreground" />
                        <span className="font-mono text-xs">{span.doc_id}</span>
                      </div>
                      <div className="text-xs text-muted-foreground mb-1">
                        Hash: {span.span_hash?.substring(0, 16)}...
                      </div>
                      <div className="text-sm">
                        {span.text}
                      </div>
                    </div>
                  ))}
                </div>
              </>
            ) : (
              <div className="text-center py-8 text-muted-foreground">
                <FileText className="h-8 w-8 mx-auto mb-2 opacity-20" />
                <p>No evidence spans available</p>
              </div>
            )}
          </TabsContent>

          {/* Performance Metrics */}
          <TabsContent value="performance" className="space-y-3">
            <div className="grid grid-cols-2 gap-4">
              <div className="p-4 bg-muted rounded-lg">
                <div className="flex items-center gap-2 mb-1">
                  <Clock className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Total Latency</span>
                </div>
                <div className="text-2xl font-bold">
                  {trace.latency_ms}ms
                </div>
              </div>

              <div className="p-4 bg-muted rounded-lg">
                <div className="flex items-center gap-2 mb-1">
                  <Target className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Router Decisions</span>
                </div>
                <div className="text-2xl font-bold">
                  {trace.router_decisions?.length || 0}
                </div>
              </div>

              <div className="p-4 bg-muted rounded-lg">
                <div className="flex items-center gap-2 mb-1">
                  <FileText className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Evidence Spans</span>
                </div>
                <div className="text-2xl font-bold">
                  {trace.evidence_spans?.length || 0}
                </div>
              </div>

              <div className="p-4 bg-muted rounded-lg">
                <div className="flex items-center gap-2 mb-1">
                  <Zap className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Avg Adapters/Token</span>
                </div>
                <div className="text-2xl font-bold">
                  {trace.router_decisions && trace.router_decisions.length > 0
                    ? (
                        trace.router_decisions.reduce(
                          (sum, d) => sum + (d.adapters?.length || 0),
                          0
                        ) / trace.router_decisions.length
                      ).toFixed(1)
                    : '0'}
                </div>
              </div>
            </div>

            {/* Performance Timeline */}
            <div className="p-4 bg-muted rounded-lg">
              <h4 className="text-sm font-medium mb-3">Performance Breakdown</h4>
              <div className="space-y-2">
                <div className="flex items-center justify-between text-sm">
                  <span>Total Time</span>
                  <span className="font-mono">{trace.latency_ms}ms</span>
                </div>
                {trace.router_decisions && trace.router_decisions.length > 0 && (
                  <div className="flex items-center justify-between text-sm">
                    <span>Avg Time/Decision</span>
                    <span className="font-mono">
                      {(trace.latency_ms / trace.router_decisions.length).toFixed(2)}ms
                    </span>
                  </div>
                )}
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>
  );
}
