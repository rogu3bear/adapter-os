import React from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { MetricsGrid } from '@/components/observability/MetricsGrid';
import { TraceTimeline } from '@/components/observability/TraceTimeline';
import { LogStream } from '@/components/observability/LogStream';

export function ObservabilityDashboard() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Observability</h1>
          <p className="text-muted-foreground mt-1">
            Live metrics, traces, and logs from the system
          </p>
        </div>
      </div>

      <Tabs defaultValue="metrics" className="w-full">
        <TabsList>
          <TabsTrigger value="metrics">Metrics</TabsTrigger>
          <TabsTrigger value="traces">Traces</TabsTrigger>
          <TabsTrigger value="logs">Logs</TabsTrigger>
        </TabsList>

        <TabsContent value="metrics" className="space-y-4">
          <MetricsGrid />
        </TabsContent>

        <TabsContent value="traces" className="space-y-4">
          <TraceTimeline />
        </TabsContent>

        <TabsContent value="logs" className="space-y-4">
          <LogStream />
        </TabsContent>
      </Tabs>
    </div>
  );
}
