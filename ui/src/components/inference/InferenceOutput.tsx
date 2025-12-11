import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { TraceVisualizer } from '@/components/TraceVisualizer';
import {
  Zap,
  Copy,
  Clock,
  FileText,
  BarChart3,
  CheckCircle,
  Target,
  TrendingUp,
  Radio,
  HelpCircle
} from 'lucide-react';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { InferResponse } from '@/api/types';

export interface InferenceMetrics {
  latency: number;
  tokensPerSecond: number;
  totalTokens: number;
}

export interface InferenceOutputProps {
  response: InferResponse | null;
  isLoading: boolean;
  metrics?: InferenceMetrics | null;
  isStreaming?: boolean;
  onCopy?: (text: string) => void;
}

export function InferenceOutput({
  response,
  isLoading,
  metrics,
  isStreaming = false,
  onCopy
}: InferenceOutputProps) {
  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    onCopy?.(text);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="text-center space-y-2">
          <Zap className="h-8 w-8 animate-pulse mx-auto text-primary" />
          <p className="text-sm text-muted-foreground">Generating response...</p>
        </div>
      </div>
    );
  }

  if (!response) {
    return (
      <div className="flex items-center justify-center p-8 text-muted-foreground">
        <FileText className="h-8 w-8 mr-2" />
        <p>No response yet. Click "Generate" to run inference.</p>
      </div>
    );
  }

  // Check if actively streaming (finish_reason is null while streaming)
  const isActivelyStreaming = isStreaming && response.finish_reason === null;

  return (
    <div className="space-y-4" data-cy="inference-output">
      <Card data-cy="inference-result">
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-base flex items-center gap-2">
              {isActivelyStreaming && <Radio className="h-4 w-4 text-green-500 animate-pulse" />}
              {isStreaming && !isActivelyStreaming && <CheckCircle className="h-4 w-4 text-green-500" />}
              Response
              {isActivelyStreaming && (
                <Badge variant="secondary" className="text-xs animate-pulse">
                  Streaming...
                </Badge>
              )}
            </CardTitle>
            <div className="flex gap-2">
              <Badge variant="outline" className="gap-1" data-cy="latency">
                <Clock className="h-3 w-3" />
                {response.latency_ms || ('trace' in response && response.trace && typeof response.trace === 'object' && response.trace !== null && 'latency_ms' in response.trace ? (response.trace as { latency_ms: number }).latency_ms : 0)}ms
              </Badge>
              <Badge variant="outline" className="gap-1" data-cy="token-usage">
                <FileText className="h-3 w-3" />
                {response.token_count || 0} tokens
              </Badge>
              {metrics && (
                <Badge variant="outline" className="gap-1">
                  <TrendingUp className="h-3 w-3" />
                  {metrics.tokensPerSecond.toFixed(1)} tokens/sec
                </Badge>
              )}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="relative">
            <pre className="whitespace-pre-wrap text-sm p-4 bg-muted border border-border rounded-lg min-h-[100px]">
              {response.text}
              {isActivelyStreaming && (
                <span className="inline-block w-2 h-4 bg-primary animate-pulse ml-0.5" />
              )}
            </pre>
            <Button
              variant="ghost"
              size="sm"
              className="absolute top-2 right-2"
              onClick={() => handleCopy(response.text)}
              disabled={isActivelyStreaming}
            >
              <Copy className="h-4 w-4" aria-hidden="true" />
            </Button>
          </div>
        </CardContent>
      </Card>

      {response.citations && response.citations.length > 0 && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm flex items-center gap-2">
              <FileText className="h-4 w-4" />
              Citations
              <Badge variant="secondary" className="text-xs">
                {response.citations.length}
              </Badge>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            {response.citations.map((citation, idx) => (
              <details
                key={`${citation.chunk_id}-${idx}`}
                className="border rounded-md p-3 space-y-1"
              >
                <summary className="cursor-pointer flex items-center justify-between gap-2 text-sm">
                  <span className="truncate">{citation.file_path}</span>
                  <span className="text-xs text-muted-foreground">
                    bytes {citation.offset_start}–{citation.offset_end}
                  </span>
                </summary>
                <div className="text-xs text-muted-foreground whitespace-pre-wrap">
                  {citation.preview}
                </div>
              </details>
            ))}
          </CardContent>
        </Card>
      )}

      {response.trace && typeof response.trace === 'object' && response.trace !== null && 'latency_ms' in response.trace && (
        <TraceVisualizer trace={response.trace as { latency_ms: number }} />
      )}

      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="flex items-center gap-2">
          <CheckCircle className="h-4 w-4 text-muted-foreground" />
          <div>
            <div className="text-sm font-medium flex items-center gap-1">
              Finish Reason
              <GlossaryTooltip brief="Indicates why generation stopped - 'stop' (complete), 'length' (max tokens reached), 'error' (failure)">
                <HelpCircle className="h-3 w-3 text-muted-foreground cursor-help" />
              </GlossaryTooltip>
            </div>
            <div className="text-xs text-muted-foreground">{response.finish_reason || 'unknown'}</div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Target className="h-4 w-4 text-muted-foreground" />
          <div>
            <div className="text-sm font-medium flex items-center gap-1">
              Router Decisions
              <GlossaryTooltip brief="Number of adapter selection decisions made during inference. Each token may trigger routing to select which adapters to use.">
                <HelpCircle className="h-3 w-3 text-muted-foreground cursor-help" />
              </GlossaryTooltip>
            </div>
            <div className="text-xs text-muted-foreground">
              {response.trace?.router_decisions?.length || 0} steps
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <BarChart3 className="h-4 w-4 text-muted-foreground" />
          <div>
            <div className="text-sm font-medium flex items-center gap-1">
              Evidence Spans
              <GlossaryTooltip brief="Document excerpts used to support the answer (RAG - Retrieval-Augmented Generation)">
                <HelpCircle className="h-3 w-3 text-muted-foreground cursor-help" />
              </GlossaryTooltip>
            </div>
            <div className="text-xs text-muted-foreground">
              {response.trace?.evidence_spans?.length || 0} found
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
