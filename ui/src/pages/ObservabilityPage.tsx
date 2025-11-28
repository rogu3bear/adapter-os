import FeatureLayout from '@/layout/FeatureLayout';
import { ObservabilityDashboard } from '@/components/ObservabilityDashboard';
import { DensityProvider } from '@/contexts/DensityContext';

export default function ObservabilityPage() {
  return (
    <DensityProvider pageKey="observability">
      <FeatureLayout
        title="Observability"
        description="Live metrics, traces, and logs"
      >
        <ObservabilityDashboard />
      </FeatureLayout>
    </DensityProvider>
  );
}

