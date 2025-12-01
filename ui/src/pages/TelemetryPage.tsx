import { useAuth, useTenant } from '@/layout/LayoutProvider';
import { useRBAC } from '@/hooks/useRBAC';
import FeatureLayout from '@/layout/FeatureLayout';
import { Telemetry } from '@/components/Telemetry';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

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
