import React from 'react';
import { apiClient } from '@/api/client';
import type { BaseModelStatus, AllModelsStatusResponse } from '@/api/types';
import { CheckCircle, XCircle, Loader2, AlertCircle } from 'lucide-react';
import { logger } from '@/utils/logger';
import { usePolling } from '@/hooks/usePolling';
import { DashboardWidgetFrame, type DashboardWidgetState } from './DashboardWidgetFrame';

interface ModelStatusBadgeProps {
  status: BaseModelStatus['status'];
}

function ModelStatusBadge({ status }: ModelStatusBadgeProps) {
  switch (status) {
    case 'ready':
      return (
        <div className="flex items-center gap-1.5 text-green-600">
          <CheckCircle className="h-4 w-4" />
          <span className="text-sm font-medium">Ready</span>
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
    case 'no-model':
      return (
        <div className="flex items-center gap-1.5 text-gray-500">
          <XCircle className="h-4 w-4" />
          <span className="text-sm font-medium">No Model</span>
        </div>
      );
    case 'error':
      return (
        <div className="flex items-center gap-1.5 text-red-600">
          <AlertCircle className="h-4 w-4" />
          <span className="text-sm font-medium">Error</span>
        </div>
      );
    case 'checking':
      return (
        <div className="flex items-center gap-1.5 text-blue-600">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span className="text-sm font-medium">Checking</span>
        </div>
      );
  }
}

export const MultiModelStatusWidget: React.FC = () => {
  const { data: status, isLoading, error, lastUpdated, refetch } = usePolling(
    () => apiClient.getAllModelsStatus(),
    'slow',
    {
      operationName: 'MultiModelStatusWidget.getAllModelsStatus',
      showLoadingIndicator: false,
      onError: (err) => {
        logger.error('Failed to fetch all models status', { component: 'MultiModelStatusWidget' }, err);
      }
    }
  );

  // Use fallback data if status is null
  const statusData: AllModelsStatusResponse = status ?? { models: [], total_memory_mb: 0, active_model_count: 0, schema_version: "v1" };

  const models = statusData.models;
  const totalMemoryMb = statusData.total_memory_mb;

  const loadedModels = models.filter(m => m.is_loaded);
  const loadingModels = models.filter(m => m.status === 'loading' || m.status === 'unloading');
  const errorModels = models.filter(m => m.status === 'error');

  const state: DashboardWidgetState = error
    ? 'error'
    : isLoading && models.length === 0
      ? 'loading'
      : models.length === 0
        ? 'empty'
        : 'ready';

  return (
    <DashboardWidgetFrame
      title="Loaded Models"
      subtitle="Model load state and memory usage"
      state={state}
      onRefresh={async () => { await refetch(); }}
      onRetry={async () => { await refetch(); }}
      lastUpdated={lastUpdated}
      errorMessage={error ? 'Failed to fetch model status' : undefined}
      emptyMessage="No models loaded"
      headerRight={
        <span className="text-sm font-normal text-muted-foreground">
          {loadedModels.length} active
        </span>
      }
    >
      <div className="space-y-4">
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

        <div className="space-y-3">
          {models.map((model) => (
            <div
              key={model.model_id}
              className="flex items-center justify-between p-3 rounded-lg border bg-card"
            >
              <div className="flex-1">
                <p className="font-medium text-sm" title={model.model_path || undefined}>
                  {model.model_name}
                </p>
                <p className="text-xs text-muted-foreground mt-0.5">
                  ID: {model.model_id}
                </p>
                {model.model_path && (
                  <p className="text-xs text-muted-foreground/70 mt-0.5 truncate" title={model.model_path}>
                    📁 {model.model_path}
                  </p>
                )}
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
          ))}
        </div>

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
    </DashboardWidgetFrame>
  );
};
