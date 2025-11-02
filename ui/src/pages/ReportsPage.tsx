import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { UserReportsPage } from '@/components/UserReportsPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function ReportsPage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="reports">
      <FeatureLayout title="Reports" description="Activity reports and metrics">
        <UserReportsPage tenantId={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}

