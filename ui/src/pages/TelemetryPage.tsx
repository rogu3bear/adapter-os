import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { useRBAC } from '@/hooks/useRBAC';
import FeatureLayout from '@/layout/FeatureLayout';
import { Telemetry } from '@/components/Telemetry';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { PageHeader as IaPageHeader } from '@/components/shared/PageHeader';

export default function TelemetryPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="telemetry">
      <FeatureLayout
        title="Telemetry"
        description="View and export telemetry data for audit and compliance"
        maxWidth="full"
        contentPadding="default"
        customHeader={
          <IaPageHeader
            cluster="Observe"
            title="Telemetry"
            description="View and export telemetry data for audit and compliance"
          />
        }
      >
        <SectionErrorBoundary sectionName="Telemetry">
          <Telemetry
            user={user}
            selectedTenant={selectedTenant}
          />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}
