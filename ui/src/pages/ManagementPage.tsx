import { useState, type ReactNode } from 'react';
import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ManagementPanel } from '@/components/ManagementPanel';
import { DensityProvider } from '@/contexts/DensityContext';

export default function ManagementPage() {
  const { selectedTenant } = useTenant();
  const [headerActions, setHeaderActions] = useState<ReactNode | null>(null);

  return (
    <DensityProvider pageKey="management">
      <FeatureLayout
        title="Management Panel"
        description="Unified system management, monitoring, and control"
        maxWidth="xl"
        contentPadding="default"
        headerActions={headerActions ?? undefined}
      >
        <ManagementPanel tenantId={selectedTenant} onToolbarChange={setHeaderActions} />
      </FeatureLayout>
    </DensityProvider>
  );
}

