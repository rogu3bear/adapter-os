import FeatureLayout from '@/layout/FeatureLayout';
import { RoutingInspector } from '@/components/RoutingInspector';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';

export default function RoutingPage() {
  return (
    <DensityProvider pageKey="routing">
      <FeatureLayout
        title="Routing"
        description="History and debug tools"
      >
        <Alert variant="default" className="mb-4">
          <AlertTitle className="flex items-center gap-2">
            <Badge variant="outline">Coming soon</Badge>
            Routing history
          </AlertTitle>
          <AlertDescription>
            Routing history is being folded into telemetry and replay. Use this view for previews and deep links.
          </AlertDescription>
        </Alert>
        <SectionErrorBoundary sectionName="Routing Inspector">
          <div className="space-y-4">
            <RoutingInspector />
          </div>
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}
