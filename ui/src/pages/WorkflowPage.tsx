import FeatureLayout from '@/layout/FeatureLayout';
import { WorkflowWizard } from '@/components/WorkflowWizard';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { PageHeader } from '@/components/ui/page-header';

export default function WorkflowPage() {
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="workflow">
      <FeatureLayout
        title="Getting Started"
        description="Onboarding and workflow wizard"
      >
        <div className="space-y-6">
          <PageHeader
            title="Getting Started"
            description="Onboarding and workflow wizard"
            helpContent="Step-by-step guide to configure your AdapterOS workflow"
          />
          <WorkflowWizard />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
