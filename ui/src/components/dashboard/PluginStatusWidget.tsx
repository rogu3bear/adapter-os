import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { apiClient } from '@/lib/api-client'; // assume

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
        const response = await apiClient.get('/v1/plugins');
        setPlugins(response.data.plugins || []);
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

  const getStatusColor = (enabled: boolean, status: string) => {
    if (!enabled) return 'gray';
    if (status === 'Started') return 'green';
    if (status === 'Degraded') return 'yellow';
    return 'red';
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
              <Badge variant={getStatusColor(p.enabled, p.health.status) as any}>Health: {p.health.status}</Badge>
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}
