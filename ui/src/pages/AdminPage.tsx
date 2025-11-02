import { useState, type ReactNode } from 'react';
import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ITAdminDashboard } from '@/components/ITAdminDashboard';
import { DensityProvider } from '@/contexts/DensityContext';

export default function AdminPage() {
  const { selectedTenant } = useTenant();
  const [headerActions, setHeaderActions] = useState<ReactNode | null>(null);

  return (
    <DensityProvider pageKey="admin">
      <FeatureLayout
        title="IT Admin"
        description="System administration and management"
        maxWidth="xl"
        contentPadding="default"
        headerActions={headerActions ?? undefined}
      >
        <ITAdminDashboard tenantId={selectedTenant} onToolbarChange={setHeaderActions} />
      </FeatureLayout>
    </DensityProvider>
  );
}
