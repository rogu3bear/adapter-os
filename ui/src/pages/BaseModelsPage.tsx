import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { BaseModelWidget } from '@/components/dashboard/BaseModelWidget';
import { BaseModelStatusComponent } from '@/components/BaseModelStatus';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

export default function BaseModelsPage() {
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="base-models">
      <FeatureLayout
        title="Base Models"
        description="Manage and monitor base models"
        brief="View and manage base model configurations and status"
      >
        <SectionErrorBoundary sectionName="Base Models">
          <div className="space-y-6">
            <BaseModelWidget />
            <BaseModelStatusComponent selectedTenant={selectedTenant} />
          </div>
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}
