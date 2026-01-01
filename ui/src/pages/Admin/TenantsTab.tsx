import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { TenantTable } from './TenantTable';
import { TenantFormModal } from './TenantFormModal';
import { useTenants } from '@/hooks/admin/useAdmin';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { withErrorBoundary } from '@/components/WithErrorBoundary';
import { Plus, RefreshCw } from 'lucide-react';

export function TenantsTab() {
  const { data: tenants, isLoading, error, refetch } = useTenants();
  const [createModalOpen, setCreateModalOpen] = useState(false);

  if (isLoading) {
    return <LoadingState message="Loading workspaces..." />;
  }

  if (error) {
    return (
      <ErrorRecovery
        error={error instanceof Error ? error.message : String(error)}
        onRetry={refetch}
      />
    );
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Workspaces</CardTitle>
              <CardDescription>Manage workspace configurations and settings</CardDescription>
            </div>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => refetch()}
              >
                <RefreshCw className="h-4 w-4 mr-2" />
                Refresh
              </Button>
              <Button
                size="sm"
                onClick={() => setCreateModalOpen(true)}
              >
                <Plus className="h-4 w-4 mr-2" />
                Create Workspace
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <TenantTable tenants={tenants || []} />
        </CardContent>
      </Card>

      <TenantFormModal
        open={createModalOpen}
        onOpenChange={setCreateModalOpen}
      />
    </div>
  );
}

export default withErrorBoundary(TenantsTab, 'Failed to load workspaces tab');
