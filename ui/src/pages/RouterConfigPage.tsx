import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { RouterConfigPage as RouterConfig } from '@/components/RouterConfigPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';

export default function RouterConfigPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="router-config">
      <FeatureLayout title="Router Configuration" description="Configure K-sparse LoRA routing parameters">
        <RouterConfig selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
