/**
 * ConnectionStatusIndicator - Global header indicator for connection and model status
 *
 * Shows overall connection status for all live data streams and model loading state.
 * Displayed in the app header next to the "Zero Egress" badge.
 */

import * as React from 'react';
import { Wifi, WifiOff, Activity, AlertTriangle, ChevronDown, Cpu, Loader2, ServerOff, AlertCircle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useLiveDataStatus, type OverallConnectionStatus } from '@/hooks/useLiveDataStatus';
import { useModelStatus, type ModelStatusState } from '@/hooks/useModelStatus';
import { useTenant } from '@/providers/FeatureProviders';
import { useErrorStoreSafe } from '@/stores/errorStore';

// ============================================================================
// Types
// ============================================================================

export interface ConnectionStatusIndicatorProps {
  /** Additional class names */
  className?: string;

  /** Show detailed dropdown on click */
  showDetails?: boolean;
}

// ============================================================================
// Status Configuration
// ============================================================================

const STATUS_CONFIG: Record<
  OverallConnectionStatus,
  {
    label: string;
    icon: React.ElementType;
    badgeClass: string;
    dotClass: string;
  }
> = {
  live: {
    label: 'Live',
    icon: Wifi,
    badgeClass: 'text-green-700 border-green-300 bg-green-50 hover:bg-green-100',
    dotClass: 'bg-green-500 animate-pulse',
  },
  partial: {
    label: 'Partial',
    icon: AlertTriangle,
    badgeClass: 'text-amber-700 border-amber-300 bg-amber-50 hover:bg-amber-100',
    dotClass: 'bg-amber-500',
  },
  polling: {
    label: 'Polling',
    icon: Activity,
    badgeClass: 'text-blue-700 border-blue-300 bg-blue-50 hover:bg-blue-100',
    dotClass: 'bg-blue-500',
  },
  offline: {
    label: 'Offline',
    icon: WifiOff,
    badgeClass: 'text-gray-600 border-gray-300 bg-gray-50 hover:bg-gray-100',
    dotClass: 'bg-gray-400',
  },
};

const MODEL_STATUS_CONFIG: Record<
  ModelStatusState,
  {
    label: string;
    icon: React.ElementType;
    badgeClass: string;
    dotClass: string;
  }
> = {
  'no-model': {
    label: 'No Model',
    icon: ServerOff,
    badgeClass: 'text-gray-600 border-gray-300 bg-gray-50 hover:bg-gray-100',
    dotClass: 'bg-gray-400',
  },
  loading: {
    label: 'Loading Model',
    icon: Loader2,
    badgeClass: 'text-blue-700 border-blue-300 bg-blue-50 hover:bg-blue-100',
    dotClass: 'bg-blue-500 animate-pulse',
  },
  loaded: {
    label: 'Model Ready',
    icon: Cpu,
    badgeClass: 'text-green-700 border-green-300 bg-green-50 hover:bg-green-100',
    dotClass: 'bg-green-500',
  },
  unloading: {
    label: 'Unloading',
    icon: Loader2,
    badgeClass: 'text-amber-700 border-amber-300 bg-amber-50 hover:bg-amber-100',
    dotClass: 'bg-amber-500',
  },
  error: {
    label: 'Model Error',
    icon: AlertTriangle,
    badgeClass: 'text-red-700 border-red-300 bg-red-50 hover:bg-red-100',
    dotClass: 'bg-red-500',
  },
  checking: {
    label: 'Checking',
    icon: Loader2,
    badgeClass: 'text-gray-600 border-gray-300 bg-gray-50 hover:bg-gray-100',
    dotClass: 'bg-gray-400',
  },
};

// ============================================================================
// Component
// ============================================================================

export function ConnectionStatusIndicator({
  className,
  showDetails = true,
}: ConnectionStatusIndicatorProps) {
  const { selectedTenant } = useTenant();
  const { overall, streams, connectedCount, totalStreams, reconnectAll } = useLiveDataStatus();
  const {
    status: modelStatus,
    modelName,
    isReady: modelReady,
    errorMessage: modelError,
  } = useModelStatus(selectedTenant || 'default');

  // Track background errors from error store (safe - may be null if outside provider)
  const errorStore = useErrorStoreSafe();
  const backgroundErrorCount = errorStore?.getActiveCount() ?? 0;
  const hasBackgroundErrors = backgroundErrorCount > 0;

  // Determine combined status to show
  const modelConfig = MODEL_STATUS_CONFIG[modelStatus];
  const connectionConfig = STATUS_CONFIG[overall];
  const prioritizeModelVisual =
    modelStatus === 'loading' ||
    modelStatus === 'error' ||
    modelStatus === 'unloading' ||
    modelStatus === 'no-model';

  const config = prioritizeModelVisual ? modelConfig : connectionConfig;
  const Icon = prioritizeModelVisual ? modelConfig.icon : connectionConfig.icon;
  const statusLabel = `${connectionConfig.label} | Model: ${modelConfig.label}`;

  // Simple badge without dropdown
  if (!showDetails || (totalStreams === 0 && !prioritizeModelVisual && !hasBackgroundErrors)) {
    return (
      <Badge
        variant="outline"
        className={cn('gap-1.5 text-xs font-normal cursor-default', config.badgeClass, className)}
      >
        <span className={cn('h-1.5 w-1.5 rounded-full', config.dotClass)} />
        <Icon className={cn('h-3 w-3', modelStatus === 'loading' && 'animate-spin')} />
        {statusLabel}
        {hasBackgroundErrors && (
          <span className="ml-1 flex items-center gap-0.5 text-amber-600">
            <AlertCircle className="h-3 w-3" />
            {backgroundErrorCount}
          </span>
        )}
      </Badge>
    );
  }

  // Badge with dropdown for details
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Badge
          variant="outline"
          className={cn(
            'gap-1.5 text-xs font-normal cursor-pointer select-none',
            hasBackgroundErrors && 'border-amber-300',
            config.badgeClass,
            className
          )}
        >
          <span className={cn('h-1.5 w-1.5 rounded-full', config.dotClass)} />
          <Icon className={cn('h-3 w-3', modelStatus === 'loading' && 'animate-spin')} />
          {statusLabel}
          {hasBackgroundErrors && (
            <span className="ml-0.5 flex items-center gap-0.5 text-amber-600">
              <AlertCircle className="h-3 w-3" />
              <span className="text-[10px]">{backgroundErrorCount}</span>
            </span>
          )}
          <ChevronDown className="h-3 w-3 opacity-50" />
        </Badge>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-72">
        {/* Model Status Section */}
        <DropdownMenuLabel className="flex items-center justify-between">
          <span>Model Status</span>
          <span
            className={cn(
              'text-xs font-normal',
              modelStatus === 'error'
                ? 'text-red-600'
                : modelStatus === 'loading' || modelStatus === 'unloading' || modelStatus === 'checking'
                  ? 'text-blue-600'
                  : modelReady
                    ? 'text-green-600'
                    : 'text-muted-foreground',
            )}
          >
            {modelStatus === 'ready'
              ? 'Ready'
              : modelStatus === 'loading'
                ? 'Loading...'
                : modelStatus === 'unloading'
                  ? 'Unloading...'
                  : modelStatus === 'checking'
                    ? 'Checking...'
                    : modelStatus === 'error'
                      ? 'Error'
                      : 'Not Loaded'}
          </span>
        </DropdownMenuLabel>
        
        {/* Model name if available */}
        {modelName && (
          <DropdownMenuItem disabled className="text-sm">
            <Cpu className="h-4 w-4 mr-2 text-muted-foreground" />
            {modelName}
          </DropdownMenuItem>
        )}
        
        {/* Model error if any */}
        {modelError && (
          <DropdownMenuItem disabled className="text-xs text-red-600">
            <AlertTriangle className="h-3 w-3 mr-2" />
            {modelError}
          </DropdownMenuItem>
        )}

        {/* No model hint */}
        {modelStatus === 'no-model' && (
          <DropdownMenuItem disabled className="text-xs text-muted-foreground">
            Import a model from Owner Home to get started
          </DropdownMenuItem>
        )}

        <DropdownMenuSeparator />
        
        {/* Connection Status Section */}
        <DropdownMenuLabel className="flex items-center justify-between">
          <span>Streams</span>
          <span className="text-xs font-normal text-muted-foreground">
            {connectedCount}/{totalStreams} connected
          </span>
        </DropdownMenuLabel>

        {/* Stream list */}
        {Object.entries(streams).map(([id, status]) => (
          <DropdownMenuItem key={id} className="flex items-center justify-between">
            <span className="text-sm truncate">{formatStreamId(id)}</span>
            <div className="flex items-center gap-2">
              {status.reconnecting && (
                <span className="text-xs text-amber-600">
                  Retry {status.reconnectAttempt || 1}
                </span>
              )}
              <span
                className={cn(
                  'h-2 w-2 rounded-full',
                  status.connected ? 'bg-green-500' : status.reconnecting ? 'bg-amber-500' : 'bg-gray-400'
                )}
              />
            </div>
          </DropdownMenuItem>
        ))}

        {Object.keys(streams).length === 0 && (
          <DropdownMenuItem disabled className="text-muted-foreground text-xs">
            No active streams
          </DropdownMenuItem>
        )}

        {/* Reconnect all button */}
        {overall !== 'live' && totalStreams > 0 && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem asChild>
              <Button
                variant="ghost"
                size="sm"
                onClick={reconnectAll}
                className="w-full justify-start"
              >
                <Wifi className="h-4 w-4 mr-2" />
                Reconnect All
              </Button>
            </DropdownMenuItem>
          </>
        )}

        {/* Background Errors Section */}
        {hasBackgroundErrors && errorStore && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="flex items-center justify-between">
              <span className="flex items-center gap-1.5 text-amber-600">
                <AlertCircle className="h-3.5 w-3.5" />
                Background Errors
              </span>
              <span className="text-xs font-normal text-amber-600">
                {backgroundErrorCount} {backgroundErrorCount === 1 ? 'error' : 'errors'}
              </span>
            </DropdownMenuLabel>
            <DropdownMenuItem
              className="text-xs text-muted-foreground"
              onClick={() => errorStore.clearAll()}
            >
              <AlertTriangle className="h-3 w-3 mr-2" />
              Clear all errors
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// ============================================================================
// Helpers
// ============================================================================

function formatStreamId(id: string): string {
  // Convert stream IDs like 'metrics-stream' to 'Metrics'
  return id
    .replace(/-stream$/, '')
    .replace(/-/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

export default ConnectionStatusIndicator;
