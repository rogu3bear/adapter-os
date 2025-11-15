import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { GitIntegrationPage } from '@/components/GitIntegrationPage';
import { DensityProvider } from '@/contexts/DensityContext';

export default function GitIntegrationPageWrapper() {
  const { selectedTenant } = useTenant();

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
