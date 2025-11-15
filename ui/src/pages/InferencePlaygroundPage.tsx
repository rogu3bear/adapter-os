import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { InferencePlayground } from '@/components/InferencePlayground';
import { DensityProvider } from '@/contexts/DensityContext';

export default function InferencePlaygroundPage() {
  const { selectedTenant } = useTenant();

  return (
    <DensityProvider pageKey="inference-playground">
      <FeatureLayout
        title="Inference Playground"
        description="Interactive inference testing with real-time adapter routing"
      >
        <InferencePlayground selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
