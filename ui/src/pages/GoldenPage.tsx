import FeatureLayout from '@/layout/FeatureLayout';
import { GoldenRuns } from '@/components/GoldenRuns';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

export default function GoldenPage() {

  return (
    <DensityProvider pageKey="golden">
      <FeatureLayout title="Golden" description="Baselines and summaries">
        <SectionErrorBoundary sectionName="Golden">
          <GoldenRuns />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

