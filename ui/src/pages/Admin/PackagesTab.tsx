import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { LoadingState } from '@/components/ui/loading-state';
import PageWrapper from '@/layout/PageWrapper';
import { PackageTable } from './PackageTable';
import { PackageFormModal } from './PackageFormModal';
import { useDeletePackage, useInstallPackage, usePackages, useUninstallPackage } from '@/hooks/useAdmin';
import type { AdapterPackage } from '@/api/types';
import { Plus, RefreshCw } from 'lucide-react';
import { withErrorBoundary } from '@/components/withErrorBoundary';
import { useAuth } from '@/providers/CoreProviders';

export function PackagesTab() {
  const { data: packages, isLoading, error, refetch } = usePackages();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<AdapterPackage | undefined>(undefined);
  const deletePackage = useDeletePackage();
  const installPackage = useInstallPackage();
  const uninstallPackage = useUninstallPackage();
  const { user } = useAuth();
  const tenantId = user?.tenant_id || 'default';

  if (isLoading) {
    return <LoadingState message="Loading packages..." />;
  }

  if (error) {
    return (
      <ErrorRecovery
        error={error instanceof Error ? error.message : String(error)}
        onRetry={refetch}
      />
    );
  }

  const handleDelete = async (pkg: AdapterPackage) => {
    await deletePackage.mutateAsync(pkg.id);
  };

  const handleInstall = async (pkg: AdapterPackage) => {
    if (!tenantId) return;
    await installPackage.mutateAsync({ tenantId, packageId: pkg.id });
  };

  const handleUninstall = async (pkg: AdapterPackage) => {
    if (!tenantId) return;
    await uninstallPackage.mutateAsync({ tenantId, packageId: pkg.id });
  };

  return (
    <PageWrapper
      pageKey="admin-packages"
      title="Packages"
      description="Named stacks with determinism defaults"
      maxWidth="xl"
      contentPadding="default"
    >
      <div className="space-y-4">
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>Packages</CardTitle>
                <CardDescription>Create and manage stack-bound packages</CardDescription>
              </div>
              <div className="flex gap-2">
                <Button variant="outline" size="sm" onClick={() => refetch()}>
                  <RefreshCw className="h-4 w-4 mr-2" />
                  Refresh
                </Button>
                <Button
                  size="sm"
                  onClick={() => {
                    setEditing(undefined);
                    setOpen(true);
                  }}
                >
                  <Plus className="h-4 w-4 mr-2" />
                  Create Package
                </Button>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <PackageTable
              packages={packages || []}
              onEdit={(pkg) => {
                setEditing(pkg);
                setOpen(true);
              }}
              onDelete={handleDelete}
              onInstall={handleInstall}
              onUninstall={handleUninstall}
            />
          </CardContent>
        </Card>
      </div>

      <PackageFormModal
        open={open}
        onOpenChange={(value) => {
          if (!value) {
            setEditing(undefined);
          }
          setOpen(value);
        }}
        pkg={editing}
      />
    </PageWrapper>
  );
}

export default withErrorBoundary(PackagesTab, 'Failed to load packages tab');

