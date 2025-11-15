import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { RouterConfigPage as RouterConfig } from '@/components/RouterConfigPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function RouterConfigPage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="router-config">
      <FeatureLayout title="Router Configuration" description="Configure K-sparse LoRA routing parameters">
        <RouterConfig selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
