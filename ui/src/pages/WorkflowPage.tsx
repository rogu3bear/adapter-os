import FeatureLayout from '@/layout/FeatureLayout';
import { WorkflowWizard } from '@/components/WorkflowWizard';
import { DensityProvider } from '@/contexts/DensityContext';

export default function WorkflowPage() {
  return (
    <DensityProvider pageKey="workflow">
      <FeatureLayout title="Getting Started" description="Onboarding and workflow wizard">
        <WorkflowWizard />
      </FeatureLayout>
    </DensityProvider>
  );
}

