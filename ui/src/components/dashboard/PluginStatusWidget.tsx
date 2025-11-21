import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import apiClient from '@/api/client';

interface PluginStatus {
  plugin: string;
  tenant: string;
  enabled: boolean;
  health: { status: string; details?: string };
}

export function PluginStatusWidget() {
  const [plugins, setPlugins] = useState<PluginStatus[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const fetchPlugins = async () => {
      try {
        const response = await apiClient.request<{ plugins: PluginStatus[] }>('/v1/plugins');
        setPlugins(response.plugins || []);
      } catch (error) {
        console.error('Failed to fetch plugins:', error);
      } finally {
        setLoading(false);
      }
    };

    fetchPlugins();
    const interval = setInterval(fetchPlugins, 30000); // 30s
    return () => clearInterval(interval);
  }, []);

  if (loading) return <div>Loading plugins...</div>;

  const getStatusVariant = (enabled: boolean, status: string): 'default' | 'secondary' | 'destructive' | 'outline' => {
    if (!enabled) return 'secondary';
    if (status === 'Started') return 'default';
    if (status === 'Degraded') return 'secondary';
    return 'destructive';
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Plugin Status</CardTitle>
      </CardHeader>
      <CardContent>
        {plugins.map((p, index) => (
          <div key={index} className="flex justify-between items-center py-2 border-b">
            <span>{p.plugin} ({p.tenant})</span>
            <div className="space-x-2">
              <Badge variant={p.enabled ? 'default' : 'secondary'}>Enabled: {p.enabled.toString()}</Badge>
              <Badge variant={getStatusVariant(p.enabled, p.health.status)}>Health: {p.health.status}</Badge>
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}
