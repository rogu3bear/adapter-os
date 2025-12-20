import React, { useState, useEffect, useCallback } from 'react';
import { Badge } from '@/components/ui/badge';
import { apiClient } from '@/api/services';
import { DashboardWidgetFrame, type DashboardWidgetState } from './DashboardWidgetFrame';

interface PluginStatus {
  plugin: string;
  tenant: string;
  enabled: boolean;
  health: { status: string; details?: string };
}

export function PluginStatusWidget() {
  const [plugins, setPlugins] = useState<PluginStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const fetchPlugins = useCallback(async () => {
    setError(null);
    try {
      const response = await apiClient.request<{ plugins: PluginStatus[] }>('/v1/plugins');
      setPlugins(response.plugins || []);
      setLastUpdated(new Date());
    } catch (err) {
      setError(err instanceof Error ? err : new Error('Failed to fetch plugins'));
    } finally {
      setLoading(false);
      setIsRefreshing(false);
    }
  }, []);

  useEffect(() => {
    setLoading(true);
    fetchPlugins();
    const interval = setInterval(fetchPlugins, 30000); // 30s
    return () => clearInterval(interval);
  }, [fetchPlugins]);

  const handleRefresh = async () => {
    setIsRefreshing(true);
    await fetchPlugins();
  };

  const getStatusVariant = (enabled: boolean, status: string): 'default' | 'secondary' | 'destructive' | 'outline' => {
    if (!enabled) return 'secondary';
    if (status === 'Started') return 'default';
    if (status === 'Degraded') return 'secondary';
    return 'destructive';
  };

  const state: DashboardWidgetState = error
    ? 'error'
    : loading
      ? 'loading'
      : plugins.length === 0
        ? 'empty'
        : 'ready';

  return (
    <DashboardWidgetFrame
      title="Plugin Status"
      subtitle="Plugin enablement and health"
      state={state}
      onRefresh={handleRefresh}
      onRetry={handleRefresh}
      lastUpdated={lastUpdated}
      errorMessage={error?.message}
      emptyMessage="No plugins found"
    >
      {plugins.map((p, index) => (
        <div key={index} className="flex justify-between items-center py-2 border-b">
          <span>{p.plugin} ({p.tenant})</span>
          <div className="space-x-2">
            <Badge variant={p.enabled ? 'default' : 'secondary'}>Enabled: {p.enabled.toString()}</Badge>
            <Badge variant={getStatusVariant(p.enabled, p.health.status)}>Health: {p.health.status}</Badge>
          </div>
        </div>
      ))}
    </DashboardWidgetFrame>
  );
}
