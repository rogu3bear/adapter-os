import FeatureLayout from '@/layout/FeatureLayout';
import { ObservabilityDashboard } from '@/components/ObservabilityDashboard';
import { DensityProvider } from '@/contexts/DensityContext';
import { PageHeader } from '@/components/ui/page-header';

export default function ObservabilityPage() {
  return (
    <DensityProvider pageKey="observability">
      <FeatureLayout title="Observability">
        <PageHeader
          title="Observability"
          description="Live metrics, traces, and logs"
        />
        <ObservabilityDashboard />
      </FeatureLayout>
    </DensityProvider>
  );
}

