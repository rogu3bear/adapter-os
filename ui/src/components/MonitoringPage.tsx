import React from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { MonitoringDashboard } from './MonitoringDashboard';
import { ResourceMonitor } from './ResourceMonitor';
import { RealtimeMetrics } from './RealtimeMetrics';
import { AlertsPage } from './AlertsPage';
import { DensityControls } from './ui/density-controls';
import { useDensity } from '../contexts/DensityContext';
import { useAuth } from '../providers/CoreProviders';
import { useTenant } from '../layout/LayoutProvider';

export function MonitoringPage() {
  const { density, setDensity } = useDensity();
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold">System Monitoring</h1>
        <DensityControls density={density} onDensityChange={setDensity} />
      </div>
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
          {user && <RealtimeMetrics user={user} selectedTenant={selectedTenant || 'default'} />}
        </TabsContent>
      </Tabs>
    </div>
  );
}
