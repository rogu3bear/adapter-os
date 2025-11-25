import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { Progress } from '@/components/ui/progress';
import {
  Layers,
  Box,
  Flame,
  Thermometer,
  Snowflake,
  ExternalLink,
  Boxes
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';

interface AdapterStack {
  id: string;
  name: string;
  adapter_ids?: string[];
  status?: string;
}

interface Adapter {
  id: string;
  adapter_id?: string;
  name?: string;
  lifecycle_state?: string;
  tier?: string;
}

interface StacksAdaptersCardProps {
  stacks: AdapterStack[];
  adapters: Adapter[];
  isLoading: boolean;
}

interface LifecycleStats {
  hot: number;
  warm: number;
  cold: number;
  resident: number;
  unloaded: number;
  total: number;
}

export function StacksAdaptersCard({ stacks, adapters, isLoading }: StacksAdaptersCardProps) {
  const navigate = useNavigate();

  const calculateLifecycleStats = (): LifecycleStats => {
    const stats: LifecycleStats = {
      hot: 0,
      warm: 0,
      cold: 0,
      resident: 0,
      unloaded: 0,
      total: adapters.length
    };

    adapters.forEach((adapter) => {
      const state = adapter.lifecycle_state?.toLowerCase();
      if (state === 'hot') stats.hot++;
      else if (state === 'warm') stats.warm++;
      else if (state === 'cold') stats.cold++;
      else if (state === 'resident') stats.resident++;
      else if (state === 'unloaded') stats.unloaded++;
    });

    return stats;
  };

  const lifecycleStats = isLoading ? null : calculateLifecycleStats();

  const getStateIcon = (state: string) => {
    switch (state) {
      case 'hot':
        return <Flame className="h-4 w-4" />;
      case 'warm':
        return <Thermometer className="h-4 w-4" />;
      case 'cold':
        return <Snowflake className="h-4 w-4" />;
      case 'resident':
        return <Boxes className="h-4 w-4" />;
      default:
        return <Box className="h-4 w-4" />;
    }
  };

  const getStateColor = (state: string): string => {
    switch (state) {
      case 'hot':
        return 'text-red-600 bg-red-50';
      case 'warm':
        return 'text-orange-600 bg-orange-50';
      case 'cold':
        return 'text-blue-600 bg-blue-50';
      case 'resident':
        return 'text-purple-600 bg-purple-50';
      default:
        return 'text-slate-600 bg-slate-50';
    }
  };

  const calculatePercentage = (count: number, total: number): number => {
    return total > 0 ? (count / total) * 100 : 0;
  };

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Layers className="h-5 w-5" />
            Stacks & Adapters
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="grid grid-cols-2 gap-4">
            <Skeleton className="h-20" />
            <Skeleton className="h-20" />
          </div>
          <Skeleton className="h-32" />
          <Skeleton className="h-10" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Layers className="h-5 w-5" />
          Stacks & Adapters
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-6">
        {/* Summary Grid */}
        <div className="grid grid-cols-2 gap-4">
          <div className="flex flex-col gap-1 p-4 bg-slate-50 rounded-lg">
            <div className="flex items-center gap-2 text-sm text-slate-600">
              <Layers className="h-4 w-4" />
              Total Stacks
            </div>
            <div className="text-2xl font-bold text-slate-900">
              {stacks.length}
            </div>
          </div>
          <div className="flex flex-col gap-1 p-4 bg-slate-50 rounded-lg">
            <div className="flex items-center gap-2 text-sm text-slate-600">
              <Box className="h-4 w-4" />
              Total Adapters
            </div>
            <div className="text-2xl font-bold text-slate-900">
              {adapters.length}
            </div>
          </div>
        </div>

        {/* Lifecycle State Distribution */}
        {lifecycleStats && lifecycleStats.total > 0 && (
          <div className="space-y-3">
            <h4 className="text-sm font-medium text-slate-700">
              Lifecycle State Distribution
            </h4>
            <div className="space-y-3">
              {/* Hot State */}
              {lifecycleStats.hot > 0 && (
                <div className="space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <div className="flex items-center gap-2">
                      <Badge
                        variant="outline"
                        className={`${getStateColor('hot')} border-0`}
                      >
                        {getStateIcon('hot')}
                        <span className="ml-1">Hot</span>
                      </Badge>
                      <span className="text-slate-600">
                        {lifecycleStats.hot} adapter{lifecycleStats.hot !== 1 ? 's' : ''}
                      </span>
                    </div>
                    <span className="font-medium text-slate-700">
                      {calculatePercentage(lifecycleStats.hot, lifecycleStats.total).toFixed(0)}%
                    </span>
                  </div>
                  <Progress
                    value={calculatePercentage(lifecycleStats.hot, lifecycleStats.total)}
                    className="h-2 bg-red-100"
                  />
                </div>
              )}

              {/* Warm State */}
              {lifecycleStats.warm > 0 && (
                <div className="space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <div className="flex items-center gap-2">
                      <Badge
                        variant="outline"
                        className={`${getStateColor('warm')} border-0`}
                      >
                        {getStateIcon('warm')}
                        <span className="ml-1">Warm</span>
                      </Badge>
                      <span className="text-slate-600">
                        {lifecycleStats.warm} adapter{lifecycleStats.warm !== 1 ? 's' : ''}
                      </span>
                    </div>
                    <span className="font-medium text-slate-700">
                      {calculatePercentage(lifecycleStats.warm, lifecycleStats.total).toFixed(0)}%
                    </span>
                  </div>
                  <Progress
                    value={calculatePercentage(lifecycleStats.warm, lifecycleStats.total)}
                    className="h-2 bg-orange-100"
                  />
                </div>
              )}

              {/* Cold State */}
              {lifecycleStats.cold > 0 && (
                <div className="space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <div className="flex items-center gap-2">
                      <Badge
                        variant="outline"
                        className={`${getStateColor('cold')} border-0`}
                      >
                        {getStateIcon('cold')}
                        <span className="ml-1">Cold</span>
                      </Badge>
                      <span className="text-slate-600">
                        {lifecycleStats.cold} adapter{lifecycleStats.cold !== 1 ? 's' : ''}
                      </span>
                    </div>
                    <span className="font-medium text-slate-700">
                      {calculatePercentage(lifecycleStats.cold, lifecycleStats.total).toFixed(0)}%
                    </span>
                  </div>
                  <Progress
                    value={calculatePercentage(lifecycleStats.cold, lifecycleStats.total)}
                    className="h-2 bg-blue-100"
                  />
                </div>
              )}

              {/* Resident State */}
              {lifecycleStats.resident > 0 && (
                <div className="space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <div className="flex items-center gap-2">
                      <Badge
                        variant="outline"
                        className={`${getStateColor('resident')} border-0`}
                      >
                        {getStateIcon('resident')}
                        <span className="ml-1">Resident</span>
                      </Badge>
                      <span className="text-slate-600">
                        {lifecycleStats.resident} adapter{lifecycleStats.resident !== 1 ? 's' : ''}
                      </span>
                    </div>
                    <span className="font-medium text-slate-700">
                      {calculatePercentage(lifecycleStats.resident, lifecycleStats.total).toFixed(0)}%
                    </span>
                  </div>
                  <Progress
                    value={calculatePercentage(lifecycleStats.resident, lifecycleStats.total)}
                    className="h-2 bg-purple-100"
                  />
                </div>
              )}

              {/* Unloaded State */}
              {lifecycleStats.unloaded > 0 && (
                <div className="space-y-1">
                  <div className="flex items-center justify-between text-xs">
                    <div className="flex items-center gap-2">
                      <Badge
                        variant="outline"
                        className={`${getStateColor('unloaded')} border-0`}
                      >
                        {getStateIcon('unloaded')}
                        <span className="ml-1">Unloaded</span>
                      </Badge>
                      <span className="text-slate-600">
                        {lifecycleStats.unloaded} adapter{lifecycleStats.unloaded !== 1 ? 's' : ''}
                      </span>
                    </div>
                    <span className="font-medium text-slate-700">
                      {calculatePercentage(lifecycleStats.unloaded, lifecycleStats.total).toFixed(0)}%
                    </span>
                  </div>
                  <Progress
                    value={calculatePercentage(lifecycleStats.unloaded, lifecycleStats.total)}
                    className="h-2 bg-slate-100"
                  />
                </div>
              )}
            </div>
          </div>
        )}

        {/* Empty State */}
        {lifecycleStats && lifecycleStats.total === 0 && (
          <div className="py-8 text-center text-sm text-slate-500">
            No adapters registered yet
          </div>
        )}

        {/* Action Buttons */}
        <div className="flex gap-3 pt-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => navigate('/admin/stacks')}
            className="flex-1 flex items-center justify-center gap-2"
          >
            <Layers className="h-4 w-4" />
            Manage Stacks
            <ExternalLink className="h-3 w-3 ml-auto" />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => navigate('/adapters')}
            className="flex-1 flex items-center justify-center gap-2"
          >
            <Box className="h-4 w-4" />
            View Adapters
            <ExternalLink className="h-3 w-3 ml-auto" />
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
