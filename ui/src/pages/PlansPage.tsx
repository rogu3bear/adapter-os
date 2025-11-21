import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Plans } from '@/components/Plans';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';

export default function PlansPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="plans">
      <FeatureLayout title="Build Plans" description="Manage training and deployment plans">
        <Plans selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
