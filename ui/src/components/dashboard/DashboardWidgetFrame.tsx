import React from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { RefreshCw } from 'lucide-react';

export type DashboardWidgetState = 'loading' | 'empty' | 'ready' | 'error';

interface DashboardWidgetFrameProps {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  state: DashboardWidgetState;
  lastUpdated?: Date | string | null;
  onRefresh?: () => void | Promise<void>;
  headerRight?: React.ReactNode;
  emptyMessage?: string;
  loadingContent?: React.ReactNode;
  errorContent?: React.ReactNode;
  children: React.ReactNode;
}

export function DashboardWidgetFrame({
  title,
  subtitle,
  state,
  lastUpdated,
  onRefresh,
  headerRight,
  emptyMessage = 'No data available',
  loadingContent = <div className="h-20 animate-pulse bg-muted rounded" />,
  errorContent = <div className="text-sm text-destructive">Failed to load data.</div>,
  children,
}: DashboardWidgetFrameProps) {
  const updatedLabel =
    lastUpdated instanceof Date
      ? lastUpdated.toLocaleString()
      : lastUpdated
        ? new Date(lastUpdated).toLocaleString()
        : null;

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between space-y-0 pb-4">
        <div className="space-y-1">
          <CardTitle className="flex items-center gap-2">{title}</CardTitle>
          {subtitle ? <CardDescription>{subtitle}</CardDescription> : null}
          {updatedLabel ? (
            <div className="text-xs text-muted-foreground">Updated {updatedLabel}</div>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          {headerRight}
          {onRefresh ? (
            <Button
              variant="ghost"
              size="icon"
              aria-label="Refresh"
              title="Refresh"
              onClick={() => void onRefresh()}
            >
              <RefreshCw className="h-4 w-4" />
            </Button>
          ) : null}
        </div>
      </CardHeader>
      <CardContent>
        {state === 'loading' ? (
          loadingContent
        ) : state === 'error' ? (
          errorContent
        ) : state === 'empty' ? (
          <div className="text-sm text-muted-foreground">{emptyMessage}</div>
        ) : (
          children
        )}
      </CardContent>
    </Card>
  );
}

export default DashboardWidgetFrame;

