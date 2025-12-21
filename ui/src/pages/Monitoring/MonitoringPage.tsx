import React from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { MonitoringDashboard } from '@/components/MonitoringDashboard';
import { ResourceMonitor } from '@/components/ResourceMonitor';
import { RealtimeMetrics } from '@/components/RealtimeMetrics';
import { AlertsPage } from '@/pages/Alerts/AlertsPage';
import { DensityControls } from '@/components/ui/density-controls';
import { useDensity } from '@/contexts/DensityContext';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { useRBAC } from '@/hooks/security/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { PermissionDenied } from '@/components/ui/permission-denied';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';

interface ComponentError {
  component: string;
  message: string;
}

export function MonitoringPage() {
  const { density, setDensity } = useDensity();
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();
  const [error, setError] = React.useState<string | null>(null);
  const [componentErrors, setComponentErrors] = React.useState<Record<string, string>>({});

  // Permission checks
  const canViewMetrics = can('metrics:view');
  const canManageAlerts = can('worker:manage');

  const handleRetry = () => {
    setError(null);
    setComponentErrors({});
    // Trigger re-fetch in child components by forcing remount
  };

  const handleComponentError = (component: string, message: string) => {
    setComponentErrors(prev => ({ ...prev, [component]: message }));
  };

  const clearComponentError = (component: string) => {
    setComponentErrors(prev => {
      const updated = { ...prev };
      delete updated[component];
      return updated;
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">System Monitoring</h1>
          <p className="text-muted-foreground">
            Performance metrics, resource usage, and system alerts
          </p>
        </div>
        <DensityControls density={density} onDensityChange={setDensity} />
      </div>

      {/* Global error state */}
      {error && errorRecoveryTemplates.genericError(error, handleRetry)}

      {/* Component-specific errors */}
      {Object.entries(componentErrors).map(([component, message]) => (
        <React.Fragment key={component}>
          {errorRecoveryTemplates.pollingError(
            `${component}: ${message}`,
            () => clearComponentError(component)
          )}
        </React.Fragment>
      ))}

      <Tabs defaultValue="overview">
        <TabsList>
          <GlossaryTooltip termId="monitoring-overview">
            <TabsTrigger value="overview">Overview</TabsTrigger>
          </GlossaryTooltip>
          <GlossaryTooltip termId="monitoring-resources">
            <TabsTrigger value="resources">Resources</TabsTrigger>
          </GlossaryTooltip>
          <GlossaryTooltip termId="monitoring-alerts">
            <TabsTrigger value="alerts">Alerts</TabsTrigger>
          </GlossaryTooltip>
          <GlossaryTooltip termId="monitoring-metrics">
            <TabsTrigger value="metrics">Metrics</TabsTrigger>
          </GlossaryTooltip>
        </TabsList>

        <TabsContent value="overview">
          <React.Suspense fallback={<div className="p-4">Loading dashboard...</div>}>
            <MonitoringDashboard />
          </React.Suspense>
        </TabsContent>

        <TabsContent value="resources">
          <React.Suspense fallback={<div className="p-4">Loading resources...</div>}>
            <ResourceMonitor />
          </React.Suspense>
        </TabsContent>

        <TabsContent value="alerts">
          <React.Suspense fallback={<div className="p-4">Loading alerts...</div>}>
            <AlertsPage />
          </React.Suspense>
        </TabsContent>

        <TabsContent value="metrics">
          {user && canViewMetrics && (
            <React.Suspense fallback={<div className="p-4">Loading metrics...</div>}>
              <RealtimeMetrics user={user} selectedTenant={selectedTenant || 'default'} />
            </React.Suspense>
          )}
          {user && !canViewMetrics && (
            <div className="p-4">
              <PermissionDenied
                requiredPermission="metrics:view"
                requiredRoles={['admin', 'operator', 'sre', 'compliance', 'auditor', 'viewer', 'developer']}
              />
            </div>
          )}
          {!user && errorRecoveryTemplates.genericError(
            'Authentication required to view metrics',
            handleRetry
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
