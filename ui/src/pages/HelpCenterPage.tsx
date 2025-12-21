import { useState } from 'react';
import { useTenant } from '@/providers/FeatureProviders';
import FeatureLayout from '@/layout/FeatureLayout';
import { HelpCenter } from '@/components/HelpCenter';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/security/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { withPageErrorBoundary } from '@/components/ui/with-page-error-boundary';

function HelpCenterPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();
  const [helpOpen, setHelpOpen] = useState(true);

  return (
    <DensityProvider pageKey="help">
      <FeatureLayout title="Help Center" description="Documentation and support resources">
        <SectionErrorBoundary sectionName="Help Center">
          <HelpCenter open={helpOpen} onOpenChange={setHelpOpen} />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

export default withPageErrorBoundary(HelpCenterPage, { pageName: 'Help Center' });
