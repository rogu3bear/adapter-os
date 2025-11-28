import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ReplayPanel } from '@/components/ReplayPanel';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';

export default function ReplayPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="replay">
      <FeatureLayout
        title="Replay"
        description="Deterministic verification"
        helpContent="Replay and verify deterministic execution sessions"
      >
        <div className="space-y-6">
          <ReplayPanel tenantId={selectedTenant} onSessionSelect={() => {}} />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
