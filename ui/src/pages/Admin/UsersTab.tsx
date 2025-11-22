import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { UserTable } from './UserTable';
import { UserFormModal } from './UserFormModal';
import { useUsers } from '@/hooks/useAdmin';
import { LoadingState } from '@/components/ui/loading-state';
import { ErrorRecovery } from '@/components/ui/error-recovery';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { withErrorBoundary } from '@/components/withErrorBoundary';
import { Plus, RefreshCw } from 'lucide-react';

export function UsersTab() {
  const { data: usersResponse, isLoading, error, refetch } = useUsers();
  const [createModalOpen, setCreateModalOpen] = useState(false);

  if (isLoading) {
    return <LoadingState message="Loading users..." />;
  }

  if (error) {
    return (
      <ErrorRecovery
        error={error instanceof Error ? error.message : String(error)}
        onRetry={refetch}
      />
    );
  }

  const users = usersResponse?.users || [];

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>Users</CardTitle>
              <CardDescription>Manage user accounts and role assignments</CardDescription>
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
                Create User
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <UserTable users={users} />
        </CardContent>
      </Card>

      <UserFormModal
        open={createModalOpen}
        onOpenChange={setCreateModalOpen}
      />
    </div>
  );
}

export default withErrorBoundary(UsersTab, 'Failed to load users tab');
