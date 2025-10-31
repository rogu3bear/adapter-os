// 【ui/src/contexts/DensityContext.tsx】 - Density context
import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { MonitoringPage } from '@/components/MonitoringPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function MetricsPage() {
  return (
    <RequireAuth>
      <DensityProvider pageKey="metrics">
        <FeatureLayout title="Metrics" description="System performance and health metrics">
          <MonitoringPage />
        </FeatureLayout>
      </DensityProvider>
    </RequireAuth>
  );
}
