import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { UserReportsPage } from '@/components/UserReportsPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { PageHeader } from '@/components/ui/page-header';

export default function ReportsPage() {
  const { selectedTenant } = useTenant();
  const { can } = useRBAC();

  // Check if user has permission to view reports (MetricsView available to all roles)
  if (!can('MetricsView')) {
    return (
      <DensityProvider pageKey="reports">
        <FeatureLayout title="Reports">
          <PageHeader
            title="Reports"
            description="Activity reports and metrics"
          />
          {errorRecoveryTemplates.permissionError(() => window.location.reload())}
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="reports">
      <FeatureLayout title="Reports">
        <PageHeader
          title="Reports"
          description="Activity reports and metrics"
        />
        <UserReportsPage tenantId={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}

