import FeatureLayout from '@/layout/FeatureLayout';
import { SingleFileAdapterTrainer } from '@/components/SingleFileAdapterTrainer';
import { DensityProvider } from '@/contexts/DensityContext';

export default function TrainerPage() {
  return (
    <DensityProvider pageKey="trainer">
      <FeatureLayout title="Single-File Trainer" description="Train adapters from a single file">
        <SingleFileAdapterTrainer />
      </FeatureLayout>
    </DensityProvider>
  );
}

