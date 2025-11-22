import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { TraceVisualizer } from '../TraceVisualizer';
import {
  Zap,
  Copy,
  Clock,
  FileText,
  BarChart3,
  CheckCircle,
  Target,
  TrendingUp,
  Radio
} from 'lucide-react';
import { InferResponse } from '../../api/types';

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
    <div className="space-y-4">
      <Card>
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
              <Badge variant="outline" className="gap-1">
                <Clock className="h-3 w-3" />
                {response.latency_ms || ('trace' in response && response.trace && 'latency_ms' in response.trace ? (response.trace as any).latency_ms : 0)}ms
              </Badge>
              <Badge variant="outline" className="gap-1">
                <FileText className="h-3 w-3" />
                {response.token_count || 0} tokens
              </Badge>
              {metrics && (
                <Badge variant="outline" className="gap-1">
                  <TrendingUp className="h-3 w-3" />
                  {metrics.tokensPerSecond.toFixed(1)} t/s
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

      {response.trace && 'latency_ms' in response.trace && (
        <TraceVisualizer trace={response.trace as any} />
      )}

      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="flex items-center gap-2">
          <CheckCircle className="h-4 w-4 text-muted-foreground" />
          <div>
            <div className="text-sm font-medium">Finish Reason</div>
            <div className="text-xs text-muted-foreground">{response.finish_reason || 'unknown'}</div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Target className="h-4 w-4 text-muted-foreground" />
          <div>
            <div className="text-sm font-medium">Router Decisions</div>
            <div className="text-xs text-muted-foreground">
              {response.trace?.router_decisions?.length || 0} steps
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <BarChart3 className="h-4 w-4 text-muted-foreground" />
          <div>
            <div className="text-sm font-medium">Evidence Spans</div>
            <div className="text-xs text-muted-foreground">
              {response.trace?.evidence_spans?.length || 0} found
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
