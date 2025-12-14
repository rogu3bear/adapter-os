import React from 'react';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Server, AlertTriangle, CheckCircle, Loader2 } from 'lucide-react';
import { useServiceStatus } from '@/hooks/system/useServiceStatus';
import { toast } from 'sonner';
import type { ServiceStatus } from '@/api/types';
import { DashboardWidgetFrame, type DashboardWidgetState } from './DashboardWidgetFrame';

export function ServiceStatusWidget() {
  const { status, isLoading, lastUpdated, refetch } = useServiceStatus();

  const failedServices = React.useMemo(
    () => status?.services?.filter(s => s.state === 'failed') || [],
    [status?.services]
  );
  const hasFailures = failedServices.length > 0;
  const totalServices = status?.services?.length || 0;

  // Show toast notification when services fail
  React.useEffect(() => {
    if (hasFailures && failedServices.length > 0) {
      toast.error(
        `${failedServices.length} service(s) failed to launch`,
        {
          duration: 5000,
          description: failedServices.map(s => s.name).join(', '),
        }
      );
    }
  }, [hasFailures, failedServices]);

  const state: DashboardWidgetState = isLoading
    ? 'loading'
    : !status?.services || status.services.length === 0
      ? 'empty'
      : 'ready';

  return (
    <DashboardWidgetFrame
      title={
        <div className="flex items-center gap-2">
          {hasFailures ? (
            <AlertTriangle className="h-5 w-5 text-destructive" />
          ) : (
            <Server className="h-5 w-5" />
          )}
          <span>Services</span>
        </div>
      }
      subtitle="Worker and API service health"
      state={state}
      onRefresh={() => refetch()}
      lastUpdated={lastUpdated}
      emptyMessage="No services reported yet"
      headerRight={
        state === 'ready' ? (
          <Badge variant="outline">
            {totalServices} total
          </Badge>
        ) : null
      }
      loadingContent={<div className="h-20 animate-pulse bg-muted rounded" />}
    >
      {hasFailures ? (
        <div className="space-y-3">
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>Service Launch Failures</AlertTitle>
            <AlertDescription>
              {failedServices.length} service{failedServices.length > 1 ? 's' : ''} failed to launch
            </AlertDescription>
          </Alert>
          <div className="space-y-2">
            {failedServices.map(service => (
              <div key={service.id} className="text-sm">
                <div className="flex items-center gap-2">
                  <Badge variant="destructive">{service.name}</Badge>
                  {(service.restart_count ?? 0) > 0 && (
                    <span className="text-muted-foreground">
                      ({service.restart_count} restart{service.restart_count !== 1 ? 's' : ''})
                    </span>
                  )}
                </div>
                {service.last_error && (
                  <p className="text-muted-foreground mt-1 text-xs">{service.last_error}</p>
                )}
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <CheckCircle className="h-4 w-4 text-green-600" />
            <span className="text-sm">
              All services running {totalServices > 0 && `(${totalServices} total)`}
            </span>
          </div>
          {status?.services && status.services.length > 0 && (
            <div className="flex flex-wrap gap-2 mt-2">
              {status.services.slice(0, 5).map(service => (
                <Badge
                  key={service.id}
                  variant={service.state === 'running' ? 'default' : 'secondary'}
                >
                  {service.name}
                </Badge>
              ))}
              {status.services.length > 5 && (
                <Badge variant="outline">+{status.services.length - 5} more</Badge>
              )}
            </div>
          )}
        </div>
      )}
    </DashboardWidgetFrame>
  );
}
