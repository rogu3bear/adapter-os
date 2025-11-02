import FeatureLayout from '@/layout/FeatureLayout';
import { MonitoringPage } from '@/components/MonitoringPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function MetricsPage() {
  return (
    <DensityProvider pageKey="metrics">
      <FeatureLayout title="Metrics" description="System performance and health metrics">
        <MonitoringPage />
      </FeatureLayout>
    </DensityProvider>
  );
}
