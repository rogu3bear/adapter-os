import FeatureLayout from '@/layout/FeatureLayout';
import { WorkflowWizard } from '@/components/WorkflowWizard';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';

export default function WorkflowPage() {
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="workflow">
      <FeatureLayout
        title="Getting Started"
        description="Onboarding and workflow wizard"
        helpContent="Step-by-step guide to configure your AdapterOS workflow"
      >
        <div className="space-y-6">
          <WorkflowWizard />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
