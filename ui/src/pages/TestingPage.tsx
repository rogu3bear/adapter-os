import FeatureLayout from '@/layout/FeatureLayout';
import { TestingPage as TestingPageComponent } from '@/components/TestingPage';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';

export default function TestingPage() {
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="testing">
      <FeatureLayout title="Testing" description="Compare against golden baselines">
        <TestingPageComponent />
      </FeatureLayout>
    </DensityProvider>
  );
}

