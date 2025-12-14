import React from 'react';
import { AlertTriangle, RefreshCw } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { DemoRecoveryHints } from '@/components/ui/demo-recovery-hints';

function formatErrorSummary(error: unknown): string | null {
  if (!error) return null;
  if (typeof error === 'string') return error;
  if (error instanceof Error) {
    const status = (error as { status?: number }).status;
    const code = (error as { code?: string; failure_code?: string }).failure_code ?? (error as { code?: string }).code;
    const prefixParts = [];
    if (typeof status === 'number') prefixParts.push(`HTTP ${status}`);
    if (typeof code === 'string' && code.trim()) prefixParts.push(code.trim());
    const prefix = prefixParts.length ? `${prefixParts.join(' ')}: ` : '';
    return `${prefix}${error.message || 'Request failed'}`;
  }
  return String(error);
}

export interface FetchErrorPanelProps {
  title?: string;
  description?: string;
  error?: unknown;
  onRetry?: () => void;
  className?: string;
  showDemoHints?: boolean;
}

export function FetchErrorPanel({
  title = 'Unable to reach the backend',
  description = 'The UI can’t connect to the AdapterOS control plane API.',
  error,
  onRetry,
  className,
  showDemoHints = true,
}: FetchErrorPanelProps) {
  const summary = formatErrorSummary(error);

  return (
    <Card className={cn('mx-auto w-full max-w-2xl', className)} role="alert" aria-live="assertive">
      <CardHeader>
        <div className="flex items-start gap-3">
          <div className="mt-0.5 flex h-9 w-9 items-center justify-center rounded-full bg-destructive/10">
            <AlertTriangle className="h-5 w-5 text-destructive" aria-hidden="true" />
          </div>
          <div className="space-y-1">
            <CardTitle className="text-lg">{title}</CardTitle>
            <CardDescription>{description}</CardDescription>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {summary && (
          <div className="text-sm text-muted-foreground">
            <span className="font-medium text-foreground">Error:</span>{' '}
            <span className="font-mono break-all">{summary}</span>
          </div>
        )}

        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={onRetry ?? (() => window.location.reload())}>
            <RefreshCw className="mr-2 h-4 w-4" />
            Retry
          </Button>
        </div>

        {showDemoHints && <DemoRecoveryHints />}
      </CardContent>
    </Card>
  );
}

