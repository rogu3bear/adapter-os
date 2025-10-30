import { RequireAuth } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { Telemetry } from '@/components/Telemetry';

export default function TelemetryPage() {
  return (
    <RequireAuth>
      <FeatureLayout title="Telemetry" description="System telemetry and event logs">
        <Telemetry />
      </FeatureLayout>
    </RequireAuth>
  );
}
