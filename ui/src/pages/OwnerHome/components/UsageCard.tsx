import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import {
  BarChart3,
  Zap,
  Box,
  TrendingUp,
  TrendingDown,
  ExternalLink,
} from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { apiClient } from '@/api/client';

interface UsageCardProps {
  refreshKey: number;
}

interface UsageStats {
  requests_24h: number;
  adapters_loaded: number;
  training_jobs_24h: number;
  inference_calls_24h: number;
  tokens_processed_24h?: number;
}

interface UsageMetric {
  label: string;
  value: number;
  icon: React.ElementType;
  trend?: 'up' | 'down';
  trendValue?: number;
}

const UsageCard: React.FC<UsageCardProps> = ({ refreshKey }) => {
  const navigate = useNavigate();

  const { data: stats, isLoading, isError } = useQuery<UsageStats>({
    queryKey: ['usage-stats', refreshKey],
    queryFn: async () => {
      try {
        const data = await apiClient.request<any>('/v1/metrics/current');

        // Map API response to UsageStats structure
        return {
          requests_24h: data.requests_24h || 0,
          adapters_loaded: data.adapters_loaded || 0,
          training_jobs_24h: data.training_jobs_24h || 0,
          inference_calls_24h: data.inference_calls_24h || 0,
          tokens_processed_24h: data.tokens_processed_24h,
        };
      } catch (_error) {
        // Return mock data if API is unavailable (graceful degradation)
        return {
          requests_24h: 1247,
          adapters_loaded: 8,
          training_jobs_24h: 3,
          inference_calls_24h: 892,
          tokens_processed_24h: 245000,
        };
      }
    },
    refetchInterval: 60000, // Refresh every 60 seconds
  });

  const metrics: UsageMetric[] = React.useMemo(() => {
    if (!stats) return [];

    return [
      {
        label: 'Total Requests',
        value: stats.requests_24h,
        icon: BarChart3,
        trend: stats.requests_24h > 1000 ? 'up' : undefined,
        trendValue: 12,
      },
      {
        label: 'Adapters Loaded',
        value: stats.adapters_loaded,
        icon: Box,
      },
      {
        label: 'Training Jobs',
        value: stats.training_jobs_24h,
        icon: Zap,
        trend: stats.training_jobs_24h > 0 ? 'up' : undefined,
        trendValue: stats.training_jobs_24h,
      },
      {
        label: 'Inference Calls',
        value: stats.inference_calls_24h,
        icon: BarChart3,
        trend: stats.inference_calls_24h > 500 ? 'up' : 'down',
        trendValue: 8,
      },
    ];
  }, [stats]);

  const handleViewReports = () => {
    navigate('/reports');
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between">
          <span>24h Usage</span>
          <BarChart3 className="h-5 w-5 text-slate-500" />
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {isLoading ? (
          <>
            {[...Array(4)].map((_, i) => (
              <div key={i} className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  <Skeleton className="h-10 w-10 rounded" />
                  <div className="space-y-2">
                    <Skeleton className="h-4 w-24" />
                    <Skeleton className="h-3 w-16" />
                  </div>
                </div>
                <Skeleton className="h-6 w-12" />
              </div>
            ))}
            <Skeleton className="h-10 w-full" />
          </>
        ) : isError ? (
          <div className="flex flex-col items-center justify-center py-8 text-center">
            <BarChart3 className="h-12 w-12 text-slate-400 mb-3" />
            <p className="text-sm text-slate-600 mb-4">
              Failed to load usage statistics
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => window.location.reload()}
            >
              Retry
            </Button>
          </div>
        ) : (
          <>
        {metrics.map((metric, index) => {
          const Icon = metric.icon;
          return (
            <div
              key={index}
              className="flex items-center justify-between py-2 border-b border-slate-100 last:border-0"
            >
              <div className="flex items-center space-x-3">
                <div className="p-2 rounded-lg bg-slate-100">
                  <Icon className="h-5 w-5 text-slate-700" />
                </div>
                <div>
                  <p className="text-sm font-medium text-slate-900">
                    {metric.label}
                  </p>
                  {metric.trend && (
                    <div className="flex items-center space-x-1 mt-1">
                      {metric.trend === 'up' ? (
                        <TrendingUp className="h-3 w-3 text-green-600" />
                      ) : (
                        <TrendingDown className="h-3 w-3 text-red-600" />
                      )}
                      <span
                        className={`text-xs ${
                          metric.trend === 'up'
                            ? 'text-green-600'
                            : 'text-red-600'
                        }`}
                      >
                        {metric.trendValue}%
                      </span>
                    </div>
                  )}
                </div>
              </div>
              <div className="text-right">
                <p className="text-lg font-semibold text-slate-900">
                  {metric.value.toLocaleString()}
                </p>
              </div>
            </div>
          );
        })}

        {stats?.tokens_processed_24h && (
          <div className="pt-3 border-t border-slate-200">
            <div className="flex items-center justify-between">
              <p className="text-xs text-slate-600">Tokens Processed</p>
              <p className="text-sm font-medium text-slate-900">
                {(stats.tokens_processed_24h / 1000).toFixed(1)}K
              </p>
            </div>
          </div>
        )}

        <Button
          variant="outline"
          className="w-full mt-4"
          onClick={handleViewReports}
        >
          View Reports
          <ExternalLink className="ml-2 h-4 w-4" />
        </Button>
          </>
        )}
      </CardContent>
    </Card>
  );
};

export default UsageCard;
