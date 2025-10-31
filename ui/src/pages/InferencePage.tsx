// 【ui/src/contexts/DensityContext.tsx】 - Density context
import { RequireAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { InferencePlayground } from '@/components/InferencePlayground';
import { DensityProvider } from '@/contexts/DensityContext';

export default function InferencePage() {
  const { selectedTenant } = useTenant();

  return (
    <RequireAuth>
      <DensityProvider pageKey="inference">
        <FeatureLayout title="Inference" description="Run inference with loaded adapters">
          <InferencePlayground selectedTenant={selectedTenant} />
        </FeatureLayout>
      </DensityProvider>
    </RequireAuth>
  );
}
