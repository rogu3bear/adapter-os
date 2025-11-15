import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Server, AlertTriangle, CheckCircle, Loader2 } from 'lucide-react';
import { usePolling } from '@/hooks/usePolling';
import apiClient from '@/api/client';
import { logger, toError } from '@/utils/logger';
import { toast } from 'sonner';
import type { AdapterOSStatus, ServiceStatus } from '@/api/types';

export function ServiceStatusWidget() {
  const { data: status, isLoading } = usePolling<AdapterOSStatus>(
    () => apiClient.getStatus(),
    'fast', // Citation: ui/src/hooks/usePolling.ts L22-26 - fast = 2000ms
    {
      operationName: 'ServiceStatusWidget.getStatus',
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Failed to fetch service status', { component: 'ServiceStatusWidget' }, toError(err));
      }
    }
  );

  const failedServices = status?.services?.filter(s => s.state === 'failed') || [];
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
  }, [hasFailures, failedServices.length]);

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Services</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="h-20 animate-pulse bg-muted rounded" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          {hasFailures ? (
            <AlertTriangle className="h-5 w-5 text-destructive" />
          ) : (
            <Server className="h-5 w-5" />
          )}
          Services
        </CardTitle>
      </CardHeader>
      <CardContent>
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
                    {service.restart_count > 0 && (
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
      </CardContent>
    </Card>
  );
}
