import { useAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { MonitoringPage } from '@/components/MonitoringPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { PERMISSIONS } from '@/utils/rbac';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ShieldAlert } from 'lucide-react';

export default function MetricsPage() {
  const { user } = useAuth();
  const { can } = useRBAC();

  const canViewMetrics = can(PERMISSIONS.METRICS_VIEW);

  return (
    <DensityProvider pageKey="metrics">
      <FeatureLayout title="Metrics" description="System performance and health metrics">
        {!canViewMetrics ? (
          <Alert variant="destructive">
            <ShieldAlert className="h-4 w-4" />
            <AlertDescription>
              You do not have permission to view metrics. Required permission: metrics:view
            </AlertDescription>
          </Alert>
        ) : (
          <MonitoringPage />
        )}
      </FeatureLayout>
    </DensityProvider>
  );
}
