import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Plans } from '@/components/Plans';
import { DensityProvider } from '@/contexts/DensityContext';

export default function PlansPage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="plans">
      <FeatureLayout title="Build Plans" description="Manage training and deployment plans">
        <Plans selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
