import { useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { TelemetryViewer } from '@/components/telemetry/TelemetryViewer';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';

export default function TelemetryViewerPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const [params] = useSearchParams();

  const requestId = useMemo(() => params.get('requestId') || undefined, [params]);

  return (
    <DensityProvider pageKey="telemetry-viewer">
      <FeatureLayout
        title="Telemetry Viewer"
        description="Per-session routing and token timeline using advanced metrics."
        maxWidth="full"
      >
        <SectionErrorBoundary sectionName="Telemetry Viewer">
          <TelemetryViewer
            initialRequestId={requestId}
            tenantId={selectedTenant || user?.tenant_id}
          />
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

