/**
 * LiveDataBadge - Reusable component showing live/stale data status
 *
 * Shows connection status with visual indicators:
 * - Live (SSE connected): Green dot with pulse animation
 * - Fresh/Recent: Gray timestamp
 * - Stale: Amber warning
 * - Very Stale: Red warning with refresh prompt
 */

import * as React from 'react';
import { Wifi, WifiOff, RefreshCw } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import type { DataFreshnessLevel, ConnectionStatus } from '@/hooks/useLiveData';

// ============================================================================
// Types
// ============================================================================

export interface LiveDataBadgeProps {
  /** Whether SSE is connected */
  isLive?: boolean;

  /** Current connection status */
  connectionStatus?: ConnectionStatus;

  /** Data freshness level */
  freshnessLevel?: DataFreshnessLevel;

  /** Timestamp of last update */
  lastUpdated?: Date | null;

  /** Called when user clicks reconnect */
  onReconnect?: () => void;

  /** Compact mode - just shows dot indicator */
  compact?: boolean;

  /** Additional class names */
  className?: string;
}

// ============================================================================
// Helpers
// ============================================================================

function formatTimeSince(date: Date): string {
  const seconds = Math.floor((Date.now() - date.getTime()) / 1000);

  if (seconds < 10) return 'just now';
  if (seconds < 60) return `${seconds}s ago`;

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;

  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function formatFullTimestamp(date: Date): string {
  return date.toLocaleString();
}

// ============================================================================
// Component
// ============================================================================

export function LiveDataBadge({
  isLive = false,
  connectionStatus = 'disconnected',
  freshnessLevel = 'stale',
  lastUpdated,
  onReconnect,
  compact = false,
  className,
}: LiveDataBadgeProps) {
  // Determine display state
  const isConnected = isLive || connectionStatus === 'sse';
  const isPolling = connectionStatus === 'polling';
  const isStale = freshnessLevel === 'stale' || freshnessLevel === 'very_stale';

  // Compact mode - just a dot indicator
  if (compact) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            className={cn(
              'inline-block h-2 w-2 rounded-full',
              isConnected && 'bg-green-500 animate-pulse',
              isPolling && 'bg-amber-500',
              !isConnected && !isPolling && 'bg-gray-400',
              className
            )}
          />
        </TooltipTrigger>
        <TooltipContent>
          {isConnected && 'Live updates active'}
          {isPolling && `Polling mode${lastUpdated ? ` - Updated ${formatTimeSince(lastUpdated)}` : ''}`}
          {!isConnected && !isPolling && 'Disconnected'}
        </TooltipContent>
      </Tooltip>
    );
  }

  // Live/SSE connected state
  if (isConnected) {
    return (
      <Badge
        variant="outline"
        className={cn(
          'gap-1.5 text-xs font-normal text-green-700 border-green-300 bg-green-50',
          className
        )}
      >
        <Wifi className="h-3 w-3" />
        <span className="h-1.5 w-1.5 rounded-full bg-green-500 animate-pulse" />
        Live
      </Badge>
    );
  }

  // Polling with recent data
  if (isPolling && !isStale && lastUpdated) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge
            variant="outline"
            className={cn('gap-1.5 text-xs font-normal text-gray-600 border-gray-300', className)}
          >
            <RefreshCw className="h-3 w-3" />
            {formatTimeSince(lastUpdated)}
          </Badge>
        </TooltipTrigger>
        <TooltipContent>{formatFullTimestamp(lastUpdated)}</TooltipContent>
      </Tooltip>
    );
  }

  // Stale data warning
  if (isStale) {
    return (
      <div className={cn('flex items-center gap-2', className)}>
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge
              variant="outline"
              className={cn(
                'gap-1.5 text-xs font-normal',
                freshnessLevel === 'very_stale'
                  ? 'text-red-700 border-red-300 bg-red-50'
                  : 'text-amber-700 border-amber-300 bg-amber-50'
              )}
            >
              <WifiOff className="h-3 w-3" />
              {lastUpdated ? formatTimeSince(lastUpdated) : 'No data'}
            </Badge>
          </TooltipTrigger>
          <TooltipContent>
            {lastUpdated
              ? `Last updated: ${formatFullTimestamp(lastUpdated)}`
              : 'No data received yet'}
          </TooltipContent>
        </Tooltip>
        {onReconnect && (
          <Button
            variant="ghost"
            size="sm"
            onClick={onReconnect}
            className="h-6 px-2 text-xs"
          >
            Reconnect
          </Button>
        )}
      </div>
    );
  }

  // Disconnected state
  return (
    <div className={cn('flex items-center gap-2', className)}>
      <Badge
        variant="outline"
        className={cn('gap-1.5 text-xs font-normal text-gray-500 border-gray-300', className)}
      >
        <WifiOff className="h-3 w-3" />
        Offline
      </Badge>
      {onReconnect && (
        <Button
          variant="ghost"
          size="sm"
          onClick={onReconnect}
          className="h-6 px-2 text-xs"
        >
          Connect
        </Button>
      )}
    </div>
  );
}

export default LiveDataBadge;
