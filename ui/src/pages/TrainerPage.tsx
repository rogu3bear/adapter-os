import FeatureLayout from '@/layout/FeatureLayout';
import { SingleFileAdapterTrainer } from '@/components/SingleFileAdapterTrainer';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';

export default function TrainerPage() {
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="trainer">
      <FeatureLayout title="Single-File Trainer" description="Train adapters from a single file">
        <SingleFileAdapterTrainer />
      </FeatureLayout>
    </DensityProvider>
  );
}

