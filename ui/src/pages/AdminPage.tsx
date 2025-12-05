import { useState, type ReactNode } from 'react';
import { useTenant } from '@/providers/FeatureProviders';
import FeatureLayout from '@/layout/FeatureLayout';
import { ITAdminDashboard } from '@/components/ITAdminDashboard';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';

export default function AdminPage() {
  const { selectedTenant } = useTenant();
  const [headerActions, setHeaderActions] = useState<ReactNode | null>(null);
  const { can, userRole } = useRBAC();

  // Check if user has admin permissions
  if (!can('TenantManage') && userRole !== 'admin') {
    return (
      <DensityProvider pageKey="admin">
        <FeatureLayout
          title="IT Admin"
          description="System administration and management"
          maxWidth="xl"
          contentPadding="default"
        >
          {errorRecoveryTemplates.permissionError(() => window.location.reload())}
        </FeatureLayout>
      </DensityProvider>
    );
  }

  return (
    <DensityProvider pageKey="admin">
      <FeatureLayout
        title="IT Admin"
        description="System administration and management"
        maxWidth="xl"
        contentPadding="default"
      >
        <ITAdminDashboard tenantId={selectedTenant} onToolbarChange={setHeaderActions} />
      </FeatureLayout>
    </DensityProvider>
  );
}
