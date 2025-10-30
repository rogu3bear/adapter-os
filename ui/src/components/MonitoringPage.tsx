import React from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { MonitoringDashboard } from './MonitoringDashboard';
import { ResourceMonitor } from './ResourceMonitor';
import { RealtimeMetrics } from './RealtimeMetrics';
import { AlertsPage } from './AlertsPage';

export function MonitoringPage() {
  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold">System Monitoring</h1>
      <Tabs defaultValue="overview">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="resources">Resources</TabsTrigger>
          <TabsTrigger value="alerts">Alerts</TabsTrigger>
          <TabsTrigger value="metrics">Metrics</TabsTrigger>
        </TabsList>
        
        <TabsContent value="overview">
          <MonitoringDashboard />
        </TabsContent>
        
        <TabsContent value="resources">
          <ResourceMonitor />
        </TabsContent>
        
        <TabsContent value="alerts">
          <AlertsPage />
        </TabsContent>
        
        <TabsContent value="metrics">
          <RealtimeMetrics user={{} as any} selectedTenant="default" />
        </TabsContent>
      </Tabs>
    </div>
  );
}
