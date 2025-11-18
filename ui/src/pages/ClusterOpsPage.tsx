import { useState, type ReactNode } from 'react';
import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ClusterOpsPanel } from '@/components/cluster/ClusterOpsPanel';
import { DensityProvider } from '@/contexts/DensityContext';

export default function ClusterOpsPage() {
  const { selectedTenant } = useTenant();
  const [headerActions, setHeaderActions] = useState<ReactNode | null>(null);

  return (
    <DensityProvider pageKey="cluster-ops">
      <FeatureLayout
        title="Node & Cluster Operations"
        description="Manage compute infrastructure and monitor cluster health"
        maxWidth="xl"
        contentPadding="default"
        headerActions={headerActions ?? undefined}
      >
        <ClusterOpsPanel tenantId={selectedTenant} onToolbarChange={setHeaderActions} />
      </FeatureLayout>
    </DensityProvider>
  );
}
