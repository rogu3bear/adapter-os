import FeatureLayout from '@/layout/FeatureLayout';
import { RoutingInspector } from '@/components/RoutingInspector';
import { DensityProvider } from '@/contexts/DensityContext';
import { SectionErrorBoundary } from '@/components/ui/section-error-boundary';

export default function RoutingPage() {
  return (
    <DensityProvider pageKey="routing">
      <FeatureLayout
        title="Routing"
        description="History and debug tools"
      >
        <SectionErrorBoundary sectionName="Routing Inspector">
          <div className="space-y-4">
            <RoutingInspector />
          </div>
        </SectionErrorBoundary>
      </FeatureLayout>
    </DensityProvider>
  );
}

