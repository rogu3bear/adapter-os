import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { apiClient } from '../../api/client';
import type { BaseModelStatus, AllModelsStatusResponse } from '../../api/types';
import { CheckCircle, XCircle, Loader2, AlertCircle } from 'lucide-react';
import { logger, toError } from '../../utils/logger';
import { useEffect, useState } from 'react';

interface ModelStatusBadgeProps {
  status: BaseModelStatus['status'];
}

function ModelStatusBadge({ status }: ModelStatusBadgeProps) {
  switch (status) {
    case 'loaded':
      return (
        <div className="flex items-center gap-1.5 text-green-600">
          <CheckCircle className="h-4 w-4" />
          <span className="text-sm font-medium">Loaded</span>
        </div>
      );
    case 'loading':
      return (
        <div className="flex items-center gap-1.5 text-blue-600">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span className="text-sm font-medium">Loading</span>
        </div>
      );
    case 'unloading':
      return (
        <div className="flex items-center gap-1.5 text-yellow-600">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span className="text-sm font-medium">Unloading</span>
        </div>
      );
    case 'unloaded':
      return (
        <div className="flex items-center gap-1.5 text-gray-500">
          <XCircle className="h-4 w-4" />
          <span className="text-sm font-medium">Unloaded</span>
        </div>
      );
    case 'error':
      return (
        <div className="flex items-center gap-1.5 text-red-600">
          <AlertCircle className="h-4 w-4" />
          <span className="text-sm font-medium">Error</span>
        </div>
      );
  }
}

export const MultiModelStatusWidget: React.FC = () => {
  const [status, setStatus] = useState<AllModelsStatusResponse | null>(null);

  useEffect(() => {
    const pollStatus = async () => {
      try {
        const response = await apiClient.getAllModelsStatus();
        setStatus(response);
      } catch (err) {
        logger.error('Failed to fetch all models status', { component: 'MultiModelStatusWidget' }, toError(err));
        setStatus({ models: [], total_memory_mb: 0, active_model_count: 0 } as AllModelsStatusResponse);
      }
    };

    pollStatus();
    const interval = setInterval(pollStatus, 10000); // 10 seconds

    return () => clearInterval(interval);
  }, []);

  if (status === null) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Loaded Models</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="h-32 animate-pulse bg-muted rounded" />
        </CardContent>
      </Card>
    );
  }

  // No explicit error field in AllModelsStatusResponse; errors are handled via logging and defaults

  const models = status.models;
  const totalMemoryMb = status.total_memory_mb;

  const loadedModels = models.filter(m => m.is_loaded);
  const loadingModels = models.filter(m => m.status === 'loading' || m.status === 'unloading');
  const errorModels = models.filter(m => m.status === 'error');

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>Loaded Models</span>
          <span className="text-sm font-normal text-muted-foreground">
            {loadedModels.length} active
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          {/* Summary metrics */}
          <div className="grid grid-cols-2 gap-4 pb-4 border-b">
            <div>
              <p className="text-sm text-muted-foreground">Total Models</p>
              <p className="text-2xl font-bold">{models.length}</p>
            </div>
            <div>
              <p className="text-sm text-muted-foreground">Memory Usage</p>
              <p className="text-2xl font-bold">{(totalMemoryMb / 1024).toFixed(1)} GB</p>
            </div>
          </div>

          {/* Model list */}
          <div className="space-y-3">
            {models.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">
                No models loaded
              </p>
            ) : (
              models.map((model) => (
                <div
                  key={model.model_id}
                  className="flex items-center justify-between p-3 rounded-lg border bg-card"
                >
                  <div className="flex-1">
                    <p className="font-medium text-sm">{model.model_name}</p>
                    <p className="text-xs text-muted-foreground mt-0.5">
                      ID: {model.model_id}
                    </p>
                    {model.error_message && (
                      <p className="text-xs text-destructive mt-1">
                        {model.error_message}
                      </p>
                    )}
                  </div>
                  <div className="flex items-center gap-4">
                    {model.memory_usage_mb !== undefined && (
                      <span className="text-xs text-muted-foreground">
                        {(model.memory_usage_mb / 1024).toFixed(1)} GB
                      </span>
                    )}
                    <ModelStatusBadge status={model.status} />
                  </div>
                </div>
              ))
            )}
          </div>

          {/* Status summary */}
          {(loadingModels.length > 0 || errorModels.length > 0) && (
            <div className="pt-4 border-t space-y-2">
              {loadingModels.length > 0 && (
                <p className="text-xs text-muted-foreground">
                  {loadingModels.length} model{loadingModels.length !== 1 ? 's' : ''} in transition
                </p>
              )}
              {errorModels.length > 0 && (
                <p className="text-xs text-destructive">
                  {errorModels.length} model{errorModels.length !== 1 ? 's' : ''} with errors
                </p>
              )}
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
};
