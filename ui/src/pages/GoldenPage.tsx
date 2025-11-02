import FeatureLayout from '@/layout/FeatureLayout';
import { GoldenRuns } from '@/components/GoldenRuns';
import { DensityProvider } from '@/contexts/DensityContext';

export default function GoldenPage() {
  return (
    <DensityProvider pageKey="golden">
      <FeatureLayout title="Golden" description="Baselines and summaries">
        <GoldenRuns />
      </FeatureLayout>
    </DensityProvider>
  );
}

