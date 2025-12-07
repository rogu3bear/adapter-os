/**
 * ModelStatusBar - Compact status bar showing current model state
 *
 * Displays model name, status, memory usage, and controls for load/unload.
 * Used in the operator chat-first dashboard.
 */

import React from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useModelStatus, type ModelStatusState } from '@/hooks/useModelStatus';
import { useAutoLoadModel } from '@/hooks/useAutoLoadModel';
import apiClient from '@/api/client';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';
import {
  Cpu,
  Loader2,
  Power,
  PowerOff,
  AlertCircle,
  CheckCircle,
  HardDrive,
  RefreshCw,
} from 'lucide-react';

interface ModelStatusBarProps {
  tenantId: string;
}

const statusConfig: Record<
  ModelStatusState,
  { label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline'; icon: React.ReactNode }
> = {
  'checking': {
    label: 'Checking...',
    variant: 'outline',
    icon: <Loader2 className="h-3 w-3 animate-spin" />,
  },
  'no-model': {
    label: 'No Model',
    variant: 'outline',
    icon: <AlertCircle className="h-3 w-3" />,
  },
  'loading': {
    label: 'Loading',
    variant: 'secondary',
    icon: <Loader2 className="h-3 w-3 animate-spin" />,
  },
  'ready': {
    label: 'Ready',
    variant: 'default',
    icon: <CheckCircle className="h-3 w-3" />,
  },
  'unloading': {
    label: 'Unloading',
    variant: 'secondary',
    icon: <Loader2 className="h-3 w-3 animate-spin" />,
  },
  'error': {
    label: 'Error',
    variant: 'destructive',
    icon: <AlertCircle className="h-3 w-3" />,
  },
};

function formatMemory(mb: number | null): string {
  if (mb === null) return '—';
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(1)} GB`;
  }
  return `${mb.toFixed(0)} MB`;
}

export function ModelStatusBar({ tenantId }: ModelStatusBarProps) {
  const { status, modelName, modelId, modelPath, memoryUsageMb, errorMessage, refresh } =
    useModelStatus(tenantId);
  const {
    isAutoLoading,
    autoLoadEnabled,
    toggleAutoLoad,
    loadModel,
    error: autoLoadError,
    isError: hasAutoLoadError,
    retry,
  } = useAutoLoadModel(tenantId, true);

  const [isUnloading, setIsUnloading] = React.useState(false);

  const config = statusConfig[status];
  const isLoading = status === 'loading' || isAutoLoading;
  const isOperationInProgress = isLoading || status === 'unloading' || isUnloading;

  const handleLoad = async () => {
    await loadModel();
  };

  const handleUnload = async () => {
    if (!modelId) return;

    setIsUnloading(true);
    try {
      await apiClient.unloadBaseModel(modelId);
      toast.success('Model unloaded');
      await refresh();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to unload model';
      logger.error('Failed to unload model', {
        component: 'ModelStatusBar',
        modelId,
        error: errorMessage,
      });
      toast.error(`Failed to unload: ${errorMessage}`);
    } finally {
      setIsUnloading(false);
    }
  };

  return (
    <div className="flex items-center justify-between gap-4 px-4 py-2 border-b bg-muted/30">
      {/* Left: Model info */}
      <div className="flex items-center gap-3">
        <Cpu className="h-4 w-4 text-muted-foreground" />

        {status === 'checking' ? (
          <Skeleton className="h-5 w-32" />
        ) : (
          <div className="flex items-center gap-2">
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="font-medium text-sm">
                  {modelName || (status === 'no-model' ? 'No model loaded' : 'Model')}
                </span>
              </TooltipTrigger>
              {modelName && (
                <TooltipContent>
                  <div className="space-y-1">
                    <div className="font-medium">{modelName}</div>
                    {modelId && <div className="text-xs text-muted-foreground">ID: {modelId}</div>}
                    {modelPath && <div className="text-xs text-muted-foreground">Path: {modelPath}</div>}
                  </div>
                </TooltipContent>
              )}
            </Tooltip>

            <Badge variant={config.variant} className="flex items-center gap-1">
              {config.icon}
              {config.label}
            </Badge>

            {memoryUsageMb !== null && status === 'ready' && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <div className="flex items-center gap-1 text-xs text-muted-foreground">
                    <HardDrive className="h-3 w-3" />
                    {formatMemory(memoryUsageMb)}
                  </div>
                </TooltipTrigger>
                <TooltipContent className="max-w-xs">Memory usage</TooltipContent>
              </Tooltip>
            )}

            {status === 'error' && errorMessage && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="text-xs text-destructive cursor-help">{errorMessage}</span>
                </TooltipTrigger>
                <TooltipContent className="max-w-xs">Model error details</TooltipContent>
              </Tooltip>
            )}

            {hasAutoLoadError && autoLoadError && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Badge variant="destructive" className="flex items-center gap-1 cursor-help">
                    <AlertCircle className="h-3 w-3" />
                    {autoLoadError.code === 'NETWORK_ERROR' ? 'Network error' : 'Load failed'}
                  </Badge>
                </TooltipTrigger>
                <TooltipContent>
                  {autoLoadError.message}
                  {autoLoadError.canRetry && ` (${autoLoadError.retryCount}/3 retries)`}
                </TooltipContent>
              </Tooltip>
            )}
          </div>
        )}
      </div>

      {/* Right: Controls */}
      <div className="flex items-center gap-4">
        {/* Retry button when error */}
        {hasAutoLoadError && autoLoadError?.canRetry && (
          <Button
            variant="outline"
            size="sm"
            onClick={retry}
            disabled={isOperationInProgress}
            className="gap-1"
          >
            <RefreshCw className="h-3 w-3" />
            Retry
          </Button>
        )}

        {/* Auto-load toggle */}
        <div className="flex items-center gap-2">
          <Switch
            id="auto-load"
            checked={autoLoadEnabled}
            onCheckedChange={toggleAutoLoad}
            disabled={isOperationInProgress}
            aria-label="Auto-load model on login"
          />
          <Label htmlFor="auto-load" className="text-xs text-muted-foreground cursor-pointer">
            Auto-load
          </Label>
        </div>

        {/* Load/Unload button */}
        {status === 'ready' || status === 'unloading' ? (
          <Button
            variant="outline"
            size="sm"
            onClick={handleUnload}
            disabled={isOperationInProgress}
            className="gap-1"
          >
            {isUnloading || status === 'unloading' ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <PowerOff className="h-3 w-3" />
            )}
            Unload
          </Button>
        ) : (
          <Button
            variant="default"
            size="sm"
            onClick={handleLoad}
            disabled={isOperationInProgress || status === 'checking'}
            className="gap-1"
          >
            {isLoading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Power className="h-3 w-3" />
            )}
            {isLoading ? 'Loading...' : 'Load Model'}
          </Button>
        )}
      </div>
    </div>
  );
}

export default ModelStatusBar;
