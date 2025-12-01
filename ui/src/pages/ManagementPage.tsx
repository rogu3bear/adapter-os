import { useState, type ReactNode } from 'react';
import { useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { ManagementPanel } from '@/components/ManagementPanel';
import { DensityProvider } from '@/contexts/DensityContext';
import { useRBAC } from '@/hooks/useRBAC';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';

export default function ManagementPage() {
  const { selectedTenant } = useTenant();
  const [headerActions, setHeaderActions] = useState<ReactNode | null>(null);
  const { can, userRole } = useRBAC();

  return (
    <DensityProvider pageKey="management">
      <FeatureLayout
        title="Management Panel"
        description="Unified system management, monitoring, and control"
        brief="Unified system management, monitoring, and control interface"
        maxWidth="xl"
        contentPadding="default"
      >
        <div className="space-y-6">
          <ManagementPanel tenantId={selectedTenant} onToolbarChange={setHeaderActions} />
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}

