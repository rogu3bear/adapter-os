import FeatureLayout from '@/layout/FeatureLayout';
import { TrainingPage as TrainingPageComponent } from '@/components/TrainingPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function TrainingPage() {
  return (
    <DensityProvider pageKey="training">
      <FeatureLayout title="Training" description="Manage and launch training jobs">
        <TrainingPageComponent />
      </FeatureLayout>
    </DensityProvider>
  );
}

