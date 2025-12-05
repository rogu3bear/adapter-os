import { useState, type ReactNode } from 'react';
import { useTenant } from '@/layout/LayoutProvider';
import PageWrapper from '@/layout/PageWrapper';
import { ManagementPanel } from '@/components/ManagementPanel';

export default function ManagementPage() {
  const { selectedTenant } = useTenant();
  const [headerActions, setHeaderActions] = useState<ReactNode | null>(null);

  return (
    <PageWrapper
      pageKey="management"
      title="Management Panel"
      description="Unified system management, monitoring, and control"
      brief="Unified system management, monitoring, and control interface"
      maxWidth="xl"
      contentPadding="default"
      headerActions={headerActions ?? undefined}
    >
      <div className="space-y-6">
        <ManagementPanel tenantId={selectedTenant} onToolbarChange={setHeaderActions} />
      </div>
    </PageWrapper>
  );
}

