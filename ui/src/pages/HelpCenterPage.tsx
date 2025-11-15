import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { HelpCenter } from '@/components/HelpCenter';
import { DensityProvider } from '@/contexts/DensityContext';

export default function HelpCenterPage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="help">
      <FeatureLayout title="Help Center" description="Documentation and support resources">
        <HelpCenter selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
