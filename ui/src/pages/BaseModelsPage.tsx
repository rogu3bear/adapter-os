import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { BaseModelWidget } from '@/components/dashboard/BaseModelWidget';
import { BaseModelStatusComponent } from '@/components/BaseModelStatus';
import { DensityProvider } from '@/contexts/DensityContext';

export default function BaseModelsPage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="base-models">
      <FeatureLayout title="Base Models" description="Manage and monitor base models">
        <div className="space-y-6">
          <BaseModelWidget />
          <BaseModelStatusComponent selectedTenant={selectedTenant} />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
