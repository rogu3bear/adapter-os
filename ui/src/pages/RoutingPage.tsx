import FeatureLayout from '@/layout/FeatureLayout';
import { RoutingInspector } from '@/components/RoutingInspector';
import { DensityProvider } from '@/contexts/DensityContext';
import { PageHeader } from '@/components/ui/page-header';

export default function RoutingPage() {
  return (
    <DensityProvider pageKey="routing">
      <FeatureLayout title="Routing">
        <PageHeader
          title="Routing"
          description="History and debug tools"
        />
        <div className="space-y-4">
          <RoutingInspector />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}

