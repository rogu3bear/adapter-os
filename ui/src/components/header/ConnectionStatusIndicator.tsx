/**
 * ConnectionStatusIndicator - Global header indicator for SSE connection status
 *
 * Shows overall connection status for all live data streams.
 * Displayed in the app header next to the "Zero Egress" badge.
 */

import * as React from 'react';
import { Wifi, WifiOff, Activity, AlertTriangle, ChevronDown } from 'lucide-react';
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

// ============================================================================
// Component
// ============================================================================

export function ConnectionStatusIndicator({
  className,
  showDetails = true,
}: ConnectionStatusIndicatorProps) {
  const { overall, streams, connectedCount, totalStreams, reconnectAll } = useLiveDataStatus();

  const config = STATUS_CONFIG[overall];
  const Icon = config.icon;

  // Simple badge without dropdown
  if (!showDetails || totalStreams === 0) {
    return (
      <Badge
        variant="outline"
        className={cn('gap-1.5 text-xs font-normal cursor-default', config.badgeClass, className)}
      >
        <span className={cn('h-1.5 w-1.5 rounded-full', config.dotClass)} />
        <Icon className="h-3 w-3" />
        {config.label}
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
            config.badgeClass,
            className
          )}
        >
          <span className={cn('h-1.5 w-1.5 rounded-full', config.dotClass)} />
          <Icon className="h-3 w-3" />
          {config.label}
          <ChevronDown className="h-3 w-3 opacity-50" />
        </Badge>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        <DropdownMenuLabel className="flex items-center justify-between">
          <span>Connection Status</span>
          <span className="text-xs font-normal text-muted-foreground">
            {connectedCount}/{totalStreams} streams
          </span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />

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
          <DropdownMenuItem disabled className="text-muted-foreground">
            No active streams
          </DropdownMenuItem>
        )}

        {/* Reconnect all button */}
        {overall !== 'live' && (
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
