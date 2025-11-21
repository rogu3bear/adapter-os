import { useAuth, useTenant } from '@/layout/LayoutProvider';
import { useRBAC } from '@/hooks/useRBAC';
import FeatureLayout from '@/layout/FeatureLayout';
import { Telemetry } from '@/components/Telemetry';
import { DensityProvider } from '@/contexts/DensityContext';

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
        <Telemetry
          user={user}
          selectedTenant={selectedTenant}
        />
      </FeatureLayout>
    </DensityProvider>
  );
}
