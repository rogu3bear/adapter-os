import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { CodeIntelligence } from '@/components/CodeIntelligence';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';

export default function CodeIntelligencePage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="code-intelligence">
      <FeatureLayout title="Code Intelligence" description="Repository scanning and code analysis">
        <CodeIntelligence selectedTenant={selectedTenant} />
      </FeatureLayout>
    </DensityProvider>
  );
}
