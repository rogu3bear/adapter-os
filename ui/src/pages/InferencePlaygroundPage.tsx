import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { InferencePlayground } from '@/components/InferencePlayground';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';

export default function InferencePlaygroundPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

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
