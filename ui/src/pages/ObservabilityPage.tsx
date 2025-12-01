import FeatureLayout from '@/layout/FeatureLayout';
import { ObservabilityDashboard } from '@/components/ObservabilityDashboard';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

export default function ObservabilityPage() {
  return (
    <DensityProvider pageKey="observability">
      <FeatureLayout
        title="Observability"
        description="Live metrics, traces, and logs"
      >
        <SectionErrorBoundary sectionName="Observability">
          <ObservabilityDashboard />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

