/**
 * Inference Streaming Component
 *
 * Real-time streaming inference UI that consumes the `/v1/infer/stream` SSE endpoint.
 * Displays tokens as they arrive, with connection status, timing metadata, and error handling.
 *
 * Features:
 * - Token-by-token streaming display
 * - Connection status indicators
 * - Timing metadata (tokens/sec, latency)
 * - OpenAI-compatible chat completion chunk format
 * - Stop sequences and [DONE] terminator support
 * - Error states with retry capability
 *
 * Usage:
 * ```tsx
 * <InferenceStreaming
 *   prompt="Tell me about LoRA adapters"
 *   adapters={["my-adapter"]}
 *   onComplete={(text) => console.log('Done:', text)}
 * />
 * ```
 */

import React, { useState, useCallback, useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Progress } from '@/components/ui/progress';
import { Separator } from '@/components/ui/separator';
import {
  AlertCircle,
  CheckCircle2,
  Loader2,
  Play,
  Square,
  RefreshCw,
  Zap,
  Clock,
} from 'lucide-react';
import { useInferenceStream, InferenceStreamOptions, InferenceStreamResult } from '@/hooks/streaming/useInferenceStream';
import { cn } from '@/lib/utils';

// ============================================================================
// Types
// ============================================================================

export interface InferenceStreamingProps {
  /** The prompt to send for inference */
  prompt: string;
  /** Model identifier (optional) */
  model?: string;
  /** Adapter stack to use */
  adapters?: string[];
  /** Maximum tokens to generate */
  maxTokens?: number;
  /** Sampling temperature */
  temperature?: number;
  /** Top-p sampling */
  topP?: number;
  /** Stop sequences */
  stopSequences?: string[];
  /** Callback when inference completes */
  onComplete?: (text: string) => void;
  /** Callback on error */
  onError?: (error: Error) => void;
  /** Auto-start streaming on mount */
  autoStart?: boolean;
  /** Show timing metadata */
  showTiming?: boolean;
  /** Show connection status */
  showStatus?: boolean;
  /** Custom CSS class */
  className?: string;
}

// ============================================================================
// Subcomponents
// ============================================================================

/**
 * Connection status indicator
 */
interface ConnectionStatusProps {
  isStreaming: boolean;
  connected: boolean;
  error: Error | null;
}

function ConnectionStatus({ isStreaming, connected, error }: ConnectionStatusProps) {
  if (error) {
    return (
      <Badge variant="destructive" className="gap-1">
        <AlertCircle className="h-3 w-3" />
        Error
      </Badge>
    );
  }

  if (isStreaming) {
    return (
      <Badge variant="default" className="gap-1 animate-pulse">
        <Loader2 className="h-3 w-3 animate-spin" />
        Streaming
      </Badge>
    );
  }

  if (connected) {
    return (
      <Badge variant="secondary" className="gap-1">
        <CheckCircle2 className="h-3 w-3" />
        Connected
      </Badge>
    );
  }

  return (
    <Badge variant="outline" className="gap-1">
      <div className="h-2 w-2 rounded-full bg-muted-foreground" />
      Idle
    </Badge>
  );
}

/**
 * Timing metrics display
 */
interface TimingMetricsProps {
  tokensPerSecond: number;
  latencyMs: number;
  tokenCount: number;
}

function TimingMetrics({ tokensPerSecond, latencyMs, tokenCount }: TimingMetricsProps) {
  return (
    <div className="flex flex-wrap gap-4 text-sm text-muted-foreground">
      <div className="flex items-center gap-2">
        <Zap className="h-4 w-4" />
        <span>{tokensPerSecond.toFixed(1)} tokens/sec</span>
      </div>
      <div className="flex items-center gap-2">
        <Clock className="h-4 w-4" />
        <span>{(latencyMs / 1000).toFixed(2)}s</span>
      </div>
      <div className="flex items-center gap-2">
        <span className="font-medium">{tokenCount}</span>
        <span>tokens</span>
      </div>
    </div>
  );
}

/**
 * Token display area with auto-scroll
 */
interface TokenDisplayProps {
  text: string;
  isStreaming: boolean;
  className?: string;
}

function TokenDisplay({ text, isStreaming, className }: TokenDisplayProps) {
  const scrollRef = React.useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom as tokens arrive
  React.useEffect(() => {
    if (scrollRef.current && isStreaming) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [text, isStreaming]);

  return (
    <div
      ref={scrollRef}
      className={cn(
        'relative max-h-96 overflow-y-auto rounded-md border bg-muted/30 p-4',
        className
      )}
    >
      {text ? (
        <div className="whitespace-pre-wrap font-mono text-sm">
          {text}
          {isStreaming && (
            <span className="inline-block h-4 w-1 animate-pulse bg-primary" />
          )}
        </div>
      ) : (
        <div className="flex h-32 items-center justify-center text-muted-foreground">
          <p className="text-sm">Tokens will appear here as they stream...</p>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

/**
 * Inference streaming component with real-time token display
 */
export function InferenceStreaming({
  prompt,
  model,
  adapters,
  maxTokens = 512,
  temperature = 0.7,
  topP,
  stopSequences,
  onComplete,
  onError,
  autoStart = false,
  showTiming = true,
  showStatus = true,
  className,
}: InferenceStreamingProps) {
  const [hasStarted, setHasStarted] = useState(false);

  // Configure stream options
  const streamOptions = useMemo<InferenceStreamOptions>(
    () => ({
      prompt,
      model,
      adapters,
      maxTokens,
      temperature,
      topP,
      stopSequences,
      enabled: hasStarted || autoStart,
      onComplete: (text) => {
        setHasStarted(false);
        onComplete?.(text);
      },
      onError: (error) => {
        setHasStarted(false);
        onError?.(error);
      },
    }),
    [prompt, model, adapters, maxTokens, temperature, topP, stopSequences, hasStarted, autoStart, onComplete, onError]
  );

  // Use the custom inference stream hook
  const {
    text,
    tokens,
    isStreaming,
    connected,
    error,
    start,
    stop,
    reset,
    latencyMs,
    tokensPerSecond,
    finishReason,
  } = useInferenceStream(streamOptions);

  // Handle start button click
  const handleStart = useCallback(() => {
    setHasStarted(true);
    start();
  }, [start]);

  // Handle stop button click
  const handleStop = useCallback(() => {
    stop();
    setHasStarted(false);
  }, [stop]);

  // Handle reset/retry
  const handleReset = useCallback(() => {
    reset();
    setHasStarted(false);
  }, [reset]);

  // Auto-start if configured
  React.useEffect(() => {
    if (autoStart && !hasStarted) {
      setHasStarted(true);
      start();
    }
  }, [autoStart, hasStarted, start]);

  return (
    <Card className={cn('w-full', className)}>
      <CardHeader>
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <CardTitle>Streaming Inference</CardTitle>
            <CardDescription>
              Real-time token-by-token generation from /v1/infer/stream
            </CardDescription>
          </div>
          {showStatus && (
            <ConnectionStatus
              isStreaming={isStreaming}
              connected={connected}
              error={error}
            />
          )}
        </div>
      </CardHeader>

      <CardContent className="space-y-4">
        {/* Error Display */}
        {error && (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription className="flex items-center justify-between gap-4">
              <span>{error.message}</span>
              <Button
                variant="outline"
                size="sm"
                onClick={handleReset}
                className="shrink-0"
              >
                <RefreshCw className="mr-2 h-4 w-4" />
                Retry
              </Button>
            </AlertDescription>
          </Alert>
        )}

        {/* Prompt Display */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium">Prompt</h3>
            {adapters && adapters.length > 0 && (
              <div className="flex gap-1">
                {adapters.map((adapter) => (
                  <Badge key={adapter} variant="outline" className="text-xs">
                    {adapter}
                  </Badge>
                ))}
              </div>
            )}
          </div>
          <div className="rounded-md border bg-muted/30 p-3 text-sm">
            {prompt}
          </div>
        </div>

        <Separator />

        {/* Token Display */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium">Response</h3>
            {finishReason && (
              <Badge variant="secondary" className="text-xs">
                {finishReason}
              </Badge>
            )}
          </div>
          <TokenDisplay text={text} isStreaming={isStreaming} />
        </div>

        {/* Timing Metrics */}
        {showTiming && (tokens.length > 0 || isStreaming) && (
          <>
            <Separator />
            <TimingMetrics
              tokensPerSecond={tokensPerSecond}
              latencyMs={latencyMs}
              tokenCount={tokens.length}
            />
          </>
        )}

        {/* Control Buttons */}
        <div className="flex gap-2">
          {!isStreaming ? (
            <Button
              onClick={handleStart}
              disabled={!prompt || isStreaming}
              className="gap-2"
            >
              <Play className="h-4 w-4" />
              Start Streaming
            </Button>
          ) : (
            <Button
              onClick={handleStop}
              variant="destructive"
              className="gap-2"
            >
              <Square className="h-4 w-4" />
              Stop
            </Button>
          )}

          {(text || error) && !isStreaming && (
            <Button onClick={handleReset} variant="outline" className="gap-2">
              <RefreshCw className="h-4 w-4" />
              Reset
            </Button>
          )}
        </div>

        {/* Progress Indicator */}
        {isStreaming && maxTokens && (
          <div className="space-y-1">
            <Progress
              value={(tokens.length / maxTokens) * 100}
              className="h-1"
            />
            <p className="text-xs text-muted-foreground">
              {tokens.length} / {maxTokens} tokens
            </p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ============================================================================
// Exports
// ============================================================================

export default InferenceStreaming;
