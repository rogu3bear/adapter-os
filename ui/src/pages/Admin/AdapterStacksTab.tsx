import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { StackTable } from './StackTable';
import { StackFormModal } from './StackFormModal';
import { useAdapterStacks } from '@/hooks/admin/useAdmin';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { withErrorBoundary } from '@/components/withErrorBoundary';
import { Plus, RefreshCw } from 'lucide-react';
import PageWrapper from '@/layout/PageWrapper';

export function AdapterStacksTab() {
  const { data: stacks, isLoading, error, refetch } = useAdapterStacks();
  const [createModalOpen, setCreateModalOpen] = useState(false);

  if (isLoading) {
    return <LoadingState message="Loading adapter stacks..." />;
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
    <PageWrapper
      pageKey="admin-stacks"
      title="Adapter Stacks"
      description="Manage reusable adapter combinations"
      maxWidth="xl"
      contentPadding="default"
    >
      <div className="space-y-4">
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>Adapter Stacks</CardTitle>
                <CardDescription>Manage reusable adapter combinations</CardDescription>
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
                  Create Stack
                </Button>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <StackTable stacks={stacks || []} />
          </CardContent>
        </Card>

        <StackFormModal
          open={createModalOpen}
          onOpenChange={setCreateModalOpen}
        />
      </div>
    </PageWrapper>
  );
}

export default withErrorBoundary(AdapterStacksTab, 'Failed to load adapter stacks tab');
