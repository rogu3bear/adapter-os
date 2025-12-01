import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Promotion } from '@/components/Promotion';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

export default function PromotionPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="promotion">
      <FeatureLayout title="Promotion" description="Promotion gates and approvals">
        <SectionErrorBoundary sectionName="Promotion">
          <Promotion user={user} selectedTenant={selectedTenant} />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

