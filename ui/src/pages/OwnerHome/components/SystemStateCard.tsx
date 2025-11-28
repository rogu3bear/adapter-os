/**
 * SystemStateCard Component
 *
 * Displays ground truth system state including:
 * - Memory pressure indicator with progress bar
 * - Top N adapters by memory usage
 * - Live/stale indicator
 */

import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { cn } from '@/lib/utils';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { Progress } from '@/components/ui/progress';
import { Separator } from '@/components/ui/separator';
import {
  MemoryStick,
  Wifi,
  WifiOff,
  AlertTriangle,
  ExternalLink,
  Flame,
  Thermometer,
  Snowflake,
  Pin,
  CircleOff,
  RefreshCw,
} from 'lucide-react';
import type {
  SystemStateResponse,
  MemoryPressureLevel,
  AdapterLifecycleState,
} from '@/api/system-state-types';

interface SystemStateCardProps {
  data: SystemStateResponse | null;
  isLoading: boolean;
  error: Error | null;
  isLive: boolean;
  lastUpdated: Date | null;
  onRefresh?: () => void;
}

// Pressure level colors
const PRESSURE_COLORS: Record<MemoryPressureLevel, string> = {
  low: 'bg-green-100 text-green-800 border-green-300',
  medium: 'bg-yellow-100 text-yellow-800 border-yellow-300',
  high: 'bg-orange-100 text-orange-800 border-orange-300',
  critical: 'bg-red-100 text-red-800 border-red-300',
};

// State icons for adapters
const STATE_ICONS: Record<AdapterLifecycleState, React.ReactNode> = {
  hot: <Flame className="h-3 w-3 text-red-500" />,
  warm: <Thermometer className="h-3 w-3 text-orange-500" />,
  cold: <Snowflake className="h-3 w-3 text-blue-500" />,
  resident: <Pin className="h-3 w-3 text-purple-500" />,
  unloaded: <CircleOff className="h-3 w-3 text-gray-400" />,
};

function formatTimeSince(date: Date): string {
  const seconds = Math.floor((Date.now() - date.getTime()) / 1000);
  if (seconds < 10) return 'Just now';
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  return `${Math.floor(seconds / 3600)}h ago`;
}

export function SystemStateCard({
  data,
  isLoading,
  error,
  isLive,
  lastUpdated,
  onRefresh,
}: SystemStateCardProps) {
  const navigate = useNavigate();

  const memoryUsagePercent = useMemo(() => {
    if (!data?.memory) return 0;
    const { total_mb, used_mb } = data.memory;
    return total_mb > 0 ? (used_mb / total_mb) * 100 : 0;
  }, [data?.memory]);

  const pressureLevel = data?.memory?.pressure_level ?? 'low';

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center justify-between text-base">
          <div className="flex items-center gap-2">
            <MemoryStick className="h-5 w-5" />
            System State
          </div>
          <div className="flex items-center gap-2">
            {isLive ? (
              <Badge variant="outline" className="text-xs text-green-600 border-green-300 gap-1">
                <Wifi className="h-3 w-3" />
                Live
              </Badge>
            ) : lastUpdated ? (
              <span className="text-xs text-muted-foreground flex items-center gap-1">
                <WifiOff className="h-3 w-3" />
                {formatTimeSince(lastUpdated)}
              </span>
            ) : null}
          </div>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {isLoading && !data ? (
          <>
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-3 w-full" />
            <Skeleton className="h-24 w-full" />
          </>
        ) : error && !data ? (
          <div className="flex flex-col items-center py-6 text-center">
            <AlertTriangle className="h-8 w-8 text-amber-500 mb-2" />
            <p className="text-sm text-slate-600 mb-3">Failed to load system state</p>
            {onRefresh && (
              <Button variant="outline" size="sm" onClick={onRefresh}>
                <RefreshCw className="h-4 w-4 mr-1" />
                Retry
              </Button>
            )}
          </div>
        ) : (
          <>
        {data && (
          <>
            {/* Memory Pressure Section */}
            <div className="space-y-2">
              <div className="flex justify-between items-center">
                <span className="text-sm text-muted-foreground">Memory Pressure</span>
                <Badge
                  variant="outline"
                  className={`text-xs ${PRESSURE_COLORS[pressureLevel]}`}
                >
                  {pressureLevel.toUpperCase()}
                </Badge>
              </div>
              <div className={cn(
                pressureLevel === 'critical' && '[&_[data-slot=progress-indicator]]:bg-red-500',
                pressureLevel === 'high' && '[&_[data-slot=progress-indicator]]:bg-orange-500',
                pressureLevel === 'medium' && '[&_[data-slot=progress-indicator]]:bg-yellow-500',
                pressureLevel === 'low' && '[&_[data-slot=progress-indicator]]:bg-green-500',
              )}>
                <Progress
                  value={memoryUsagePercent}
                  className="h-2"
                />
              </div>
              <div className="flex justify-between text-xs text-muted-foreground">
                <span>
                  {((data.memory.used_mb) / 1024).toFixed(1)} GB used
                </span>
                <span className="font-medium">{memoryUsagePercent.toFixed(1)}%</span>
                <span>
                  {((data.memory.total_mb) / 1024).toFixed(1)} GB total
                </span>
              </div>
              {data.memory.headroom_percent < 15 && (
                <div className="text-xs text-amber-600 flex items-center gap-1">
                  <AlertTriangle className="h-3 w-3" />
                  Low headroom ({data.memory.headroom_percent.toFixed(1)}%)
                </div>
              )}
            </div>

            <Separator />

            {/* Top Adapters Section */}
            <div className="space-y-2">
              <div className="flex justify-between items-center">
                <span className="text-sm text-muted-foreground">Top Adapters by Memory</span>
                <span className="text-xs text-muted-foreground">
                  {data.memory.top_adapters.length} shown
                </span>
              </div>
              <div className="space-y-1">
                {data.memory.top_adapters.length === 0 ? (
                  <div className="py-3 text-center text-xs text-muted-foreground">
                    No adapters loaded
                  </div>
                ) : (
                  data.memory.top_adapters.slice(0, 5).map((adapter, index) => (
                    <div
                      key={adapter.adapter_id}
                      className="flex items-center justify-between py-1.5 px-2 rounded hover:bg-slate-50 transition-colors text-sm"
                    >
                      <div className="flex items-center gap-2 min-w-0 flex-1">
                        <span className="text-xs text-muted-foreground w-4 flex-shrink-0">
                          {index + 1}.
                        </span>
                        <span className="truncate font-medium">
                          {adapter.name}
                        </span>
                      </div>
                      <div className="flex items-center gap-2 flex-shrink-0">
                        <span className="text-xs text-muted-foreground">
                          {adapter.memory_mb.toFixed(1)} MB
                        </span>
                        {STATE_ICONS[adapter.state] || STATE_ICONS.unloaded}
                      </div>
                    </div>
                  ))
                )}
              </div>
            </div>

            {/* Footer */}
            <Button
              variant="outline"
              className="w-full"
              size="sm"
              onClick={() => navigate('/system/memory')}
            >
              View Memory Details
              <ExternalLink className="h-4 w-4 ml-2" />
            </Button>
          </>
        )}
          </>
        )}
      </CardContent>
    </Card>
  );
}

export default SystemStateCard;
