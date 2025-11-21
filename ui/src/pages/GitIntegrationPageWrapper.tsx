import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { GitIntegrationPage } from '@/components/GitIntegrationPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';

export default function GitIntegrationPageWrapper() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="git-integration">
      <FeatureLayout
        title="Git Integration"
        description="Manage Git repositories, commits, and diffs"
      >
        <GitIntegrationPage selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
