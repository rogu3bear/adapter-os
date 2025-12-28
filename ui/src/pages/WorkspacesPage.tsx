import { useCallback, useMemo } from 'react';
import { toast } from 'sonner';
import { CheckCircle2, RefreshCw } from 'lucide-react';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useAuth } from '@/providers/CoreProviders';
import { useTenant } from '@/providers/FeatureProviders';
import { useWorkspaces } from '@/hooks/workspace/useWorkspaces';
import { WorkspaceCard, WorkspaceSelector } from '@/components/workspaces';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';

export default function WorkspacesPage() {
  const { user } = useAuth();
  const { selectedTenant, setSelectedTenant } = useTenant();
  const {
    workspaces,
    userWorkspaces,
    isLoading,
    error,
    updateWorkspace,
    deleteWorkspace,
    refetch,
  } = useWorkspaces({ includeMembers: true, includeResources: true });

  const availableWorkspaces = useMemo(
    () => (userWorkspaces.length > 0 ? userWorkspaces : workspaces),
    [userWorkspaces, workspaces],
  );

  const canManageWorkspaces = useMemo(
    () => ['admin', 'operator', 'sre'].includes((user?.role ?? '').toLowerCase()),
    [user?.role],
  );

  const workspaceNames = useMemo(() => {
    const map: Record<string, string> = {};
    availableWorkspaces.forEach(ws => {
      map[ws.id] = ws.name;
    });
    return map;
  }, [availableWorkspaces]);

  const handleSelect = useCallback((workspaceId: string) => {
    void (async () => {
      const ok = await setSelectedTenant(workspaceId);
      if (ok) {
        toast.success(`Workspace "${workspaceNames[workspaceId] ?? workspaceId}" selected`);
      } else {
        toast.error('Unable to switch workspace. You may not have access.');
      }
    })();
  }, [setSelectedTenant, workspaceNames]);

  const handleEdit = useCallback(async (workspaceId: string, data: { name?: string; description?: string }) => {
    const updated = await updateWorkspace(workspaceId, data);
    await refetch();
    toast.success('Workspace updated');
    return updated;
  }, [refetch, updateWorkspace]);

  const handleDelete = useCallback(async (workspaceId: string) => {
    await deleteWorkspace(workspaceId);
    await refetch();
    toast.success('Workspace removed');
  }, [deleteWorkspace, refetch]);

  return (
    <DensityProvider pageKey="workspaces">
      <FeatureLayout
        title="Workspaces"
        description="Pick the workspace used across AdapterOS."
        brief="Workspace selection cascades to chat, training, routing, and telemetry views."
        maxWidth="xl"
        contentPadding="default"
      >
        <div className="space-y-4">
          <Card>
            <CardHeader className="flex flex-row items-start justify-between gap-4">
              <div>
                <CardTitle>Active workspace</CardTitle>
                <CardDescription>Switch contexts without leaving the current session.</CardDescription>
              </div>
              <Badge variant="secondary" className="flex items-center gap-1">
                <CheckCircle2 className="h-3 w-3" />
                {selectedTenant ? workspaceNames[selectedTenant] ?? selectedTenant : 'None selected'}
              </Badge>
            </CardHeader>
            <CardContent className="space-y-3">
              <WorkspaceSelector
                workspaces={availableWorkspaces}
                selectedWorkspaceId={selectedTenant || ''}
                onWorkspaceSelect={handleSelect}
                loading={isLoading}
              />
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => refetch()}
                  disabled={isLoading}
                >
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Refresh list
                </Button>
              </div>
            </CardContent>
          </Card>

          {error && (
            <Alert variant="destructive">
              <AlertTitle>Unable to load workspaces</AlertTitle>
              <AlertDescription>{error.message}</AlertDescription>
            </Alert>
          )}

          {isLoading ? (
            <div className="grid gap-3 md:grid-cols-2">
              <Skeleton className="h-48 w-full" />
              <Skeleton className="h-48 w-full" />
            </div>
          ) : availableWorkspaces.length === 0 ? (
            <Alert>
              <AlertTitle>No workspaces available</AlertTitle>
              <AlertDescription>
                Create a workspace from the API or ask an administrator to share one with you.
              </AlertDescription>
            </Alert>
          ) : (
            <div className="grid gap-3 md:grid-cols-2">
              {availableWorkspaces.map(workspace => (
                <WorkspaceCard
                  key={workspace.id}
                  workspace={workspace}
                  onSelect={handleSelect}
                  onEdit={handleEdit}
                  onDelete={handleDelete}
                  canManage={canManageWorkspaces}
                />
              ))}
            </div>
          )}
        </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
