import FeatureLayout from '@/layout/FeatureLayout';
import { GoldenRuns } from '@/components/GoldenRuns';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

export default function GoldenPage() {
  const { can, userRole } = useRBAC();

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

