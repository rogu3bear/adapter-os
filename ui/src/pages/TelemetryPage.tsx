// 【ui/src/contexts/DensityContext.tsx】 - Density context
import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Telemetry } from '@/components/Telemetry';
import { DensityProvider } from '@/contexts/DensityContext';

export default function TelemetryPage() {
  return (
    <RequireAuth>
      <DensityProvider pageKey="telemetry">
        <FeatureLayout title="Telemetry" description="System telemetry and event logs">
          <Telemetry />
        </FeatureLayout>
      </DensityProvider>
    </RequireAuth>
  );
}
