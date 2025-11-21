import FeatureLayout from '@/layout/FeatureLayout';
import { GoldenRuns } from '@/components/GoldenRuns';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';

export default function GoldenPage() {
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="golden">
      <FeatureLayout title="Golden" description="Baselines and summaries">
        <GoldenRuns />
      </FeatureLayout>
    </DensityProvider>
  );
}

