import { RequireAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { InferencePlayground } from '@/components/InferencePlayground';

export default function InferencePage() {
  const { selectedTenant } = useTenant();

  return (
    <RequireAuth>
      <FeatureLayout title="Inference" description="Run inference with loaded adapters">
        <InferencePlayground selectedTenant={selectedTenant} />
      </FeatureLayout>
    </RequireAuth>
  );
}
