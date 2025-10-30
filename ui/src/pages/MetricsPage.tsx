import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { MonitoringPage } from '@/components/MonitoringPage';

export default function MetricsPage() {
  return (
    <RequireAuth>
      <FeatureLayout title="Metrics" description="System performance and health metrics">
        <MonitoringPage />
      </FeatureLayout>
    </RequireAuth>
  );
}
