/**
 * MetricComponents - Shared metric display components for System pages
 * Extracted to eliminate duplication between SystemOverviewPage and SystemOverviewTab
 */

import React from 'react';
import { Card, CardContent, CardHeader, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { Progress } from '@/components/ui/progress';
import type { HealthStatus } from '@/hooks/system/useSystemMetrics';

interface HealthBadgeProps {
  status: HealthStatus;
}

export function HealthBadge({ status }: HealthBadgeProps) {
  const variant = {
    healthy: 'success' as const,
    warning: 'warning' as const,
    critical: 'destructive' as const,
    unknown: 'secondary' as const,
  }[status];

  return (
    <Badge variant={variant}>
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </Badge>
  );
}

interface MetricCardProps {
  title: string;
  value: string | number;
  unit?: string;
  progress?: number;
  status?: HealthStatus;
  isLoading?: boolean;
}

export function MetricCard({
  title,
  value,
  unit,
  progress,
  status,
  isLoading,
}: MetricCardProps) {
  if (isLoading) {
    return (
      <Card>
        <CardHeader className="pb-2">
          <Skeleton className="h-4 w-24" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-8 w-16 mb-2" />
          <Skeleton className="h-2 w-full" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardDescription>{title}</CardDescription>
          {status && <HealthBadge status={status} />}
        </div>
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold">
          {value}
          {unit && <span className="text-sm font-normal text-muted-foreground ml-1">{unit}</span>}
        </div>
        {progress !== undefined && (
          <Progress value={progress} className="mt-2" />
        )}
      </CardContent>
    </Card>
  );
}
