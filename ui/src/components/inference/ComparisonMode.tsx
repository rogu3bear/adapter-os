import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Textarea } from '@/components/ui/textarea';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import {
  Play,
  Copy,
  BarChart3,
  Square,
  Zap,
  Clock,
  FileText,
  CheckCircle,
  TrendingUp,
  Target,
  Info,
  HelpCircle,
} from 'lucide-react';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { InferResponse, InferenceConfig } from '@/api/types';
import { TraceVisualizer } from '@/components/TraceVisualizer';

export interface ComparisonModeProps {
  prompt: string;
  configA: InferenceConfig;
  configB: InferenceConfig;
  responseA: InferResponse | null;
  responseB: InferResponse | null;
  isLoadingA: boolean;
  isLoadingB: boolean;
  isRunning: boolean;
  canExecute: boolean;
  metrics?: {
    latency: number;
    tokensPerSecond: number;
    totalTokens: number;
  } | null;
  onPromptChange: (prompt: string) => void;
  onConfigAChange: (config: InferenceConfig) => void;
  onConfigBChange: (config: InferenceConfig) => void;
  onRunA: () => void;
  onRunB: () => void;
  onCancel: () => void;
  onCopy: (text: string) => void;
  renderAdvancedOptions: (config: InferenceConfig, setConfig: (c: InferenceConfig) => void) => React.ReactNode;
}

export function ComparisonMode({
  prompt,
  configA,
  configB,
  responseA,
  responseB,
  isLoadingA,
  isLoadingB,
  isRunning,
  canExecute,
  metrics,
  onPromptChange,
  onConfigAChange,
  onConfigBChange,
  onRunA,
  onRunB,
  onCancel,
  onCopy,
  renderAdvancedOptions,
}: ComparisonModeProps) {
  const renderResponse = (response: InferResponse | null, isLoading: boolean) => {
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

    return (
      <div className="space-y-4">
        {/* Response Text */}
        <Card>
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base flex items-center gap-2">
                <CheckCircle className="h-4 w-4 text-green-500" />
                Response
              </CardTitle>
              <div className="flex gap-2">
                <Badge variant="outline" className="gap-1">
                  <Clock className="h-3 w-3" />
                  {response.latency_ms || ('trace' in response && response.trace && typeof response.trace === 'object' && response.trace !== null && 'latency_ms' in response.trace ? (response.trace as { latency_ms: number }).latency_ms : 0)}ms
                </Badge>
                <Badge variant="outline" className="gap-1">
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
              <pre className="whitespace-pre-wrap text-sm p-4 bg-muted border border-border rounded-lg">
                {response.text}
              </pre>
              <Button
                variant="ghost"
                size="sm"
                className="absolute top-2 right-2"
                onClick={() => onCopy(response.text)}
              >
                <Copy className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* Trace Information */}
        {response.trace && typeof response.trace === 'object' && response.trace !== null && 'latency_ms' in response.trace && (
          <TraceVisualizer trace={response.trace as { latency_ms: number }} />
        )}

        {/* Enhanced Metadata */}
        <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
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
  };

  return (
    <div className="space-y-4">
      {/* Configuration Comparison Intro */}
      <Alert>
        <Info className="h-4 w-4" />
        <AlertTitle>Configuration Comparison</AlertTitle>
        <AlertDescription>
          Test two different parameter sets against the same prompt. Compare outputs, latency, and token usage to find optimal settings.
        </AlertDescription>
      </Alert>

      {/* Shared Prompt */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Shared Prompt</CardTitle>
        </CardHeader>
        <CardContent>
          <Textarea
            placeholder="Enter prompt to compare..."
            value={prompt}
            onChange={(e) => onPromptChange(e.target.value)}
            rows={4}
          />
        </CardContent>
      </Card>

      {/* Side-by-Side Configurations */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Config A */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">Configuration A</CardTitle>
              <Badge>Temperature: {configA.temperature}</Badge>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {renderAdvancedOptions(configA, onConfigAChange)}

            <div className="flex gap-2">
              <Button
                className={`flex-1 ${!canExecute ? 'opacity-50 cursor-not-allowed' : ''}`}
                onClick={onRunA}
                disabled={isLoadingA || !prompt.trim() || !canExecute}
                title={!canExecute ? 'Requires inference:execute permission' : undefined}
              >
                <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                Generate A
              </Button>
              {isRunning && (
                <Button
                  variant="outline"
                  onClick={onCancel}
                  aria-label="Cancel inference A"
                >
                  <Square className="h-4 w-4" />
                </Button>
              )}
            </div>

            {renderResponse(responseA, isLoadingA)}
          </CardContent>
        </Card>

        {/* Config B */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">Configuration B</CardTitle>
              <Badge>Temperature: {configB.temperature}</Badge>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {renderAdvancedOptions(configB, onConfigBChange)}

            <div className="flex gap-2">
              <Button
                className={`flex-1 ${!canExecute ? 'opacity-50 cursor-not-allowed' : ''}`}
                onClick={onRunB}
                disabled={isLoadingB || !prompt.trim() || !canExecute}
                title={!canExecute ? 'Requires inference:execute permission' : undefined}
              >
                <Play className="h-4 w-4 mr-2" aria-hidden="true" />
                Generate B
              </Button>
              {isRunning && (
                <Button
                  variant="outline"
                  onClick={onCancel}
                  aria-label="Cancel inference B"
                >
                  <Square className="h-4 w-4" />
                </Button>
              )}
            </div>

            {renderResponse(responseB, isLoadingB)}
          </CardContent>
        </Card>
      </div>

      {/* Comparison Summary */}
      {responseA && responseB && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base flex items-center gap-2">
              <BarChart3 className="h-4 w-4" aria-hidden="true" />
              Comparison Summary
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div>
                <p className="text-sm font-medium">Latency</p>
                <div className="flex items-center gap-2 mt-1">
                  <Badge variant="outline">A: {responseA.latency_ms || (responseA.trace && typeof responseA.trace === 'object' && 'latency_ms' in responseA.trace ? (responseA.trace as { latency_ms: number }).latency_ms : 0)}ms</Badge>
                  <Badge variant="outline">B: {responseB.latency_ms || (responseB.trace && typeof responseB.trace === 'object' && 'latency_ms' in responseB.trace ? (responseB.trace as { latency_ms: number }).latency_ms : 0)}ms</Badge>
                </div>
              </div>
              <div>
                <p className="text-sm font-medium">Tokens</p>
                <div className="flex items-center gap-2 mt-1">
                  <Badge variant="outline">A: {responseA.token_count || ('tokens' in responseA ? responseA.tokens : 0) || 0}</Badge>
                  <Badge variant="outline">B: {responseB.token_count || ('tokens' in responseB ? responseB.tokens : 0) || 0}</Badge>
                </div>
              </div>
              <div>
                <p className="text-sm font-medium">Finish Reason</p>
                <div className="flex items-center gap-2 mt-1">
                  <Badge variant="outline">{responseA.finish_reason || 'unknown'}</Badge>
                  <Badge variant="outline">{responseB.finish_reason || 'unknown'}</Badge>
                </div>
              </div>
              <div>
                <p className="text-sm font-medium">Winner</p>
                <Badge className="mt-1">
                  {((responseA.latency_ms || (responseA.trace && typeof responseA.trace === 'object' && 'latency_ms' in responseA.trace ? (responseA.trace as { latency_ms: number }).latency_ms : 0)) < (responseB.latency_ms || (responseB.trace && typeof responseB.trace === 'object' && 'latency_ms' in responseB.trace ? (responseB.trace as { latency_ms: number }).latency_ms : 0))) ? 'A (Faster)' : 'B (Faster)'}
                </Badge>
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
