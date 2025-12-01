import { useState } from 'react';
import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { HelpCenter } from '@/components/HelpCenter';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';

export default function HelpCenterPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();
  const [helpOpen, setHelpOpen] = useState(true);

  return (
    <DensityProvider pageKey="help">
      <FeatureLayout title="Help Center" description="Documentation and support resources">
        <HelpCenter open={helpOpen} onOpenChange={setHelpOpen} />
      </FeatureLayout>
    </DensityProvider>
  );
}
