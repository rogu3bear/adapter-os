import { useState, type ReactNode } from 'react';
import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Telemetry } from '@/components/Telemetry';
import { DensityProvider } from '@/contexts/DensityContext';

export default function TelemetryPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const [headerActions, setHeaderActions] = useState<ReactNode | null>(null);

  return (
    <DensityProvider pageKey="telemetry">
      <FeatureLayout
        title="Telemetry"
        description="View and export telemetry data for audit and compliance"
        maxWidth="full"
        contentPadding="default"
        headerActions={headerActions ?? undefined}
      >
        <Telemetry
          user={user}
          selectedTenant={selectedTenant}
          onToolbarChange={setHeaderActions}
        />
      </FeatureLayout>
    </DensityProvider>
  );
}
