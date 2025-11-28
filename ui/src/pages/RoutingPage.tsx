import FeatureLayout from '@/layout/FeatureLayout';
import { RoutingInspector } from '@/components/RoutingInspector';
import { DensityProvider } from '@/contexts/DensityContext';

export default function RoutingPage() {
  return (
    <DensityProvider pageKey="routing">
      <FeatureLayout
        title="Routing"
        description="History and debug tools"
      >
        <div className="space-y-4">
          <RoutingInspector />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}

