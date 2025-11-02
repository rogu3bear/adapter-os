import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { InferencePlayground } from '@/components/InferencePlayground';
import { DensityProvider } from '@/contexts/DensityContext';

export default function InferencePage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="inference">
      <FeatureLayout title="Inference" description="Run inference with loaded adapters">
        <InferencePlayground selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
