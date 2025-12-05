import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ReplayPanel } from '@/components/ReplayPanel';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { PageHeader as IaPageHeader } from '@/components/shared/PageHeader';

export default function ReplayPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="replay">
      <FeatureLayout
        title="Replay"
        description="Deterministic verification"
        brief="Replay and verify deterministic execution sessions"
        customHeader={
          <IaPageHeader
            cluster="Verify"
            title="Replay"
            description="Deterministic verification"
            brief="Replay and verify deterministic execution sessions"
          />
        }
      >
        <div className="space-y-6">
          <SectionErrorBoundary sectionName="Replay">
            <ReplayPanel tenantId={selectedTenant} onSessionSelect={() => {}} />
          </SectionErrorBoundary>
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
