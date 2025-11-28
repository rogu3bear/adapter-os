/**
 * AdapterLoadingStatus - Shows current adapter lifecycle states in chat
 *
 * Displays the loading state of adapters in the current stack,
 * with real-time updates via SSE.
 */

import * as React from 'react';
import { Flame, Thermometer, Snowflake, Pin, CircleOff, Loader2, CheckCircle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Badge } from '@/components/ui/badge';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useSSE } from '@/hooks/useSSE';
import type { AdapterStreamEvent, AdapterStateTransitionEvent } from '@/api/streaming-types';

// ============================================================================
// Types
// ============================================================================

export type AdapterLifecycleState = 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';

export interface AdapterState {
  id: string;
  name: string;
  state: AdapterLifecycleState;
  memoryMb?: number;
  isLoading?: boolean;
  error?: string;
}

export interface AdapterLoadingStatusProps {
  /** Stack ID to monitor */
  stackId?: string;

  /** List of adapter states to display */
  adapters: AdapterState[];

  /** Called when all adapters are ready (hot or warm) */
  onAllReady?: () => void;

  /** Called when user requests to load an adapter */
  onLoadRequested?: (adapterId: string) => void;

  /** Show in compact mode */
  compact?: boolean;

  /** Additional class names */
  className?: string;
}

// ============================================================================
// Lifecycle State Configuration
// ============================================================================

const STATE_CONFIG: Record<
  AdapterLifecycleState,
  {
    label: string;
    icon: React.ElementType;
    colorClass: string;
    bgClass: string;
    description: string;
    ready: boolean;
  }
> = {
  hot: {
    label: 'Hot',
    icon: Flame,
    colorClass: 'text-red-600',
    bgClass: 'bg-red-50 border-red-200',
    description: 'Fully loaded in memory, fastest inference',
    ready: true,
  },
  warm: {
    label: 'Warm',
    icon: Thermometer,
    colorClass: 'text-orange-600',
    bgClass: 'bg-orange-50 border-orange-200',
    description: 'Partially loaded, quick activation (~2s)',
    ready: true,
  },
  cold: {
    label: 'Cold',
    icon: Snowflake,
    colorClass: 'text-blue-600',
    bgClass: 'bg-blue-50 border-blue-200',
    description: 'On disk, needs loading (~5s)',
    ready: false,
  },
  resident: {
    label: 'Resident',
    icon: Pin,
    colorClass: 'text-purple-600',
    bgClass: 'bg-purple-50 border-purple-200',
    description: 'Protected in memory, always available',
    ready: true,
  },
  unloaded: {
    label: 'Unloaded',
    icon: CircleOff,
    colorClass: 'text-gray-500',
    bgClass: 'bg-gray-50 border-gray-200',
    description: 'Not loaded, needs full initialization',
    ready: false,
  },
};

// ============================================================================
// Helper Functions
// ============================================================================

function getEstimatedLoadTime(state: AdapterLifecycleState): string {
  switch (state) {
    case 'cold':
      return '~5s';
    case 'unloaded':
      return '~10s';
    default:
      return '';
  }
}

function isAdapterReady(state: AdapterLifecycleState): boolean {
  return STATE_CONFIG[state]?.ready ?? false;
}

// ============================================================================
// Component
// ============================================================================

export function AdapterLoadingStatus({
  stackId,
  adapters,
  onAllReady,
  onLoadRequested,
  compact = false,
  className,
}: AdapterLoadingStatusProps) {
  const [adapterStates, setAdapterStates] = React.useState<Map<string, AdapterState>>(
    new Map(adapters.map((a) => [a.id, a]))
  );

  // Update states when adapters prop changes
  React.useEffect(() => {
    setAdapterStates(new Map(adapters.map((a) => [a.id, a])));
  }, [adapters]);

  // Subscribe to adapter state transitions via SSE
  const { data: sseEvent } = useSSE<AdapterStreamEvent>('/v1/stream/adapters', {
    enabled: !!stackId,
    onMessage: (event) => {
      if (event && 'current_state' in event) {
        const transition = event as AdapterStateTransitionEvent;
        setAdapterStates((prev) => {
          const updated = new Map(prev);
          const existing = updated.get(transition.adapter_id);
          if (existing) {
            updated.set(transition.adapter_id, {
              ...existing,
              state: transition.current_state,
              isLoading: false,
            });
          }
          return updated;
        });
      }
    },
  });

  // Check if all adapters are ready
  const allReady = React.useMemo(() => {
    const states = Array.from(adapterStates.values());
    return states.length > 0 && states.every((a) => isAdapterReady(a.state));
  }, [adapterStates]);

  // Notify when all ready
  React.useEffect(() => {
    if (allReady && onAllReady) {
      onAllReady();
    }
  }, [allReady, onAllReady]);

  // Count ready vs not ready
  const readyCount = Array.from(adapterStates.values()).filter((a) => isAdapterReady(a.state)).length;
  const totalCount = adapterStates.size;

  // Compact mode - just show summary
  if (compact) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge
            variant="outline"
            className={cn(
              'gap-1.5 text-xs cursor-default',
              allReady
                ? 'text-green-700 border-green-300 bg-green-50'
                : 'text-amber-700 border-amber-300 bg-amber-50',
              className
            )}
          >
            {allReady ? (
              <CheckCircle className="h-3 w-3" />
            ) : (
              <Loader2 className="h-3 w-3 animate-spin" />
            )}
            {readyCount}/{totalCount} Ready
          </Badge>
        </TooltipTrigger>
        <TooltipContent>
          <div className="space-y-1">
            {Array.from(adapterStates.values()).map((adapter) => {
              const config = STATE_CONFIG[adapter.state];
              const Icon = config.icon;
              return (
                <div key={adapter.id} className="flex items-center gap-2 text-xs">
                  <Icon className={cn('h-3 w-3', config.colorClass)} />
                  <span className="truncate max-w-[120px]">{adapter.name}</span>
                  <span className={cn('text-xs', config.colorClass)}>{config.label}</span>
                </div>
              );
            })}
          </div>
        </TooltipContent>
      </Tooltip>
    );
  }

  // Full mode - show all adapters with states
  return (
    <div className={cn('space-y-2', className)}>
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>Adapter States</span>
        <span>
          {readyCount}/{totalCount} ready
        </span>
      </div>

      <div className="space-y-1">
        {Array.from(adapterStates.values()).map((adapter) => {
          const config = STATE_CONFIG[adapter.state];
          const Icon = config.icon;
          const loadTime = getEstimatedLoadTime(adapter.state);

          return (
            <div
              key={adapter.id}
              className={cn(
                'flex items-center justify-between px-2 py-1.5 rounded border',
                config.bgClass
              )}
            >
              <div className="flex items-center gap-2">
                {adapter.isLoading ? (
                  <Loader2 className={cn('h-4 w-4 animate-spin', config.colorClass)} />
                ) : (
                  <Icon className={cn('h-4 w-4', config.colorClass)} />
                )}
                <span className="text-sm font-medium truncate max-w-[140px]">{adapter.name}</span>
              </div>

              <div className="flex items-center gap-2">
                {adapter.error ? (
                  <span className="text-xs text-red-600">{adapter.error}</span>
                ) : (
                  <>
                    <Badge variant="outline" className={cn('text-xs', config.colorClass)}>
                      {adapter.isLoading ? 'Loading...' : config.label}
                    </Badge>
                    {loadTime && !isAdapterReady(adapter.state) && (
                      <span className="text-xs text-muted-foreground">{loadTime}</span>
                    )}
                  </>
                )}

                {!isAdapterReady(adapter.state) && !adapter.isLoading && onLoadRequested && (
                  <button
                    onClick={() => onLoadRequested(adapter.id)}
                    className="text-xs text-blue-600 hover:text-blue-800 hover:underline"
                  >
                    Load
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default AdapterLoadingStatus;
