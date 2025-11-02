import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ReplayPanel } from '@/components/ReplayPanel';
import { DensityProvider } from '@/contexts/DensityContext';

export default function ReplayPage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="replay">
      <FeatureLayout title="Replay" description="Deterministic verification">
        <ReplayPanel tenantId={selectedTenant} onSessionSelect={() => {}} />
      </FeatureLayout>
    </DensityProvider>
  );
}

