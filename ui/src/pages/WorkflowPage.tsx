import FeatureLayout from '@/layout/FeatureLayout';
import { WorkflowWizard } from '@/components/WorkflowWizard';
import { DensityProvider } from '@/contexts/DensityContext';

export default function WorkflowPage() {
  return (
    <DensityProvider pageKey="workflow">
      <FeatureLayout
        title="Getting Started"
        description="Complete the onboarding checklist to finish setup"
        brief="Connect a model, run a probe, and verify evidence."
      >
        <div className="space-y-6">
          <WorkflowWizard />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
