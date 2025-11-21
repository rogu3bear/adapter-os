//! Workspaces management page
//!
//! Displays user's workspaces with CRUD operations.
//! Shows workspace members and shared resources.
//!
//! Citation: Page structure from existing pages like ui/src/components/Nodes.tsx L49-L786
//! - Table layout with create/edit/delete actions
//! - Dialog patterns from ui/src/components/Nodes.tsx

import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useWorkspaces } from '@/hooks/useWorkspaces';
import { useRBAC } from '@/hooks/useRBAC';
import { WorkspaceCard } from '@/components/WorkspaceCard';
import { WorkspaceMembers } from '@/components/WorkspaceMembers';
import { WorkspaceResources } from '@/components/WorkspaceResources';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import {
  Plus,
  Building,
  Users,
  FolderOpen,
  AlertCircle,
  RefreshCw,
  Search
} from 'lucide-react';
import { logger } from '@/utils/logger';

export default function WorkspacesPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();
  const [searchTerm, setSearchTerm] = useState('');
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [selectedWorkspace, setSelectedWorkspace] = useState<string | null>(null);

  const {
    workspaces,
    userWorkspaces,
    loading,
    error,
    createWorkspace,
    updateWorkspace,
    deleteWorkspace,
    refresh
  } = useWorkspaces({
    enabled: true,
    includeMembers: true,
    includeResources: true,
  });

  const filteredWorkspaces = userWorkspaces.filter(workspace =>
    workspace.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    (workspace.description && workspace.description.toLowerCase().includes(searchTerm.toLowerCase()))
  );

  const handleCreateWorkspace = async (data: { name: string; description?: string }) => {
    try {
      await createWorkspace({
        name: data.name,
        description: data.description,
        tenant_id: selectedTenant,
      });
      setShowCreateDialog(false);
      logger.info('Workspace created from UI', {
        component: 'WorkspacesPage',
        operation: 'create_workspace',
        workspaceName: data.name,
        tenantId: selectedTenant,
      });
    } catch (err) {
      logger.error('Failed to create workspace from UI', {
        component: 'WorkspacesPage',
        operation: 'create_workspace',
        workspaceName: data.name,
        tenantId: selectedTenant,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const handleRefresh = async () => {
    await refresh();
  };

  return (
    <DensityProvider pageKey="workspaces">
      <FeatureLayout
        title="Workspaces"
        description="Manage collaborative workspaces and shared resources"
        headerActions={
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleRefresh}
            disabled={loading}
            aria-label="Refresh workspaces"
          >
            <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
          <Button
            onClick={() => setShowCreateDialog(true)}
            className="flex items-center gap-2"
          >
            <Plus className="h-4 w-4" />
            Create Workspace
          </Button>
        </div>
      }
    >
      <div className="space-y-6">
        {/* Search */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Building className="h-5 w-5" />
              Your Workspaces
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center gap-4 mb-4">
              <div className="relative flex-1 max-w-sm">
                <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder="Search workspaces..."
                  value={searchTerm}
                  onChange={(e) => setSearchTerm(e.target.value)}
                  className="pl-9"
                />
              </div>
              <Badge variant="outline">
                {filteredWorkspaces.length} workspace{filteredWorkspaces.length !== 1 ? 's' : ''}
              </Badge>
            </div>

            {error && (
              <Alert className="mb-4">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  Failed to load workspaces: {error}
                </AlertDescription>
              </Alert>
            )}

            {loading && filteredWorkspaces.length === 0 ? (
              <div className="space-y-4">
                {Array.from({ length: 3 }).map((_, i) => (
                  <div key={i} className="h-24 bg-muted animate-pulse rounded-lg" />
                ))}
              </div>
            ) : filteredWorkspaces.length === 0 ? (
              <div className="text-center py-12">
                <Building className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
                <h3 className="text-lg font-semibold mb-2">
                  {searchTerm ? 'No workspaces found' : 'No workspaces yet'}
                </h3>
                <p className="text-muted-foreground mb-4">
                  {searchTerm
                    ? 'Try adjusting your search terms.'
                    : 'Create your first workspace to start collaborating.'
                  }
                </p>
                {!searchTerm && (
                  <Button onClick={() => setShowCreateDialog(true)}>
                    <Plus className="h-4 w-4 mr-2" />
                    Create Workspace
                  </Button>
                )}
              </div>
            ) : (
              <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
                {filteredWorkspaces.map((workspace) => (
                  <WorkspaceCard
                    key={workspace.id}
                    workspace={workspace}
                    onSelect={setSelectedWorkspace}
                    onEdit={updateWorkspace}
                    onDelete={deleteWorkspace}
                  />
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Workspace Details */}
        {selectedWorkspace && (
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <FolderOpen className="h-5 w-5" />
                Workspace Details
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Tabs defaultValue="members" className="w-full">
                <TabsList className="grid w-full grid-cols-2">
                  <TabsTrigger value="members" className="flex items-center gap-2">
                    <Users className="h-4 w-4" />
                    Members
                  </TabsTrigger>
                  <TabsTrigger value="resources" className="flex items-center gap-2">
                    <FolderOpen className="h-4 w-4" />
                    Resources
                  </TabsTrigger>
                </TabsList>

                <TabsContent value="members" className="mt-4">
                  <WorkspaceMembers workspaceId={selectedWorkspace} />
                </TabsContent>

                <TabsContent value="resources" className="mt-4">
                  <WorkspaceResources workspaceId={selectedWorkspace} />
                </TabsContent>
              </Tabs>
            </CardContent>
          </Card>
        )}

        {/* Create Workspace Dialog */}
        <CreateWorkspaceDialog
          open={showCreateDialog}
          onOpenChange={setShowCreateDialog}
          onCreate={handleCreateWorkspace}
        />
      </div>
      </FeatureLayout>
    </DensityProvider>
  );
}

// Create Workspace Dialog Component
interface CreateWorkspaceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreate: (data: { name: string; description?: string }) => Promise<void>;
}

function CreateWorkspaceDialog({ open, onOpenChange, onCreate }: CreateWorkspaceDialogProps) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [creating, setCreating] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    setCreating(true);
    try {
      await onCreate({ name: name.trim(), description: description.trim() || undefined });
      setName('');
      setDescription('');
    } catch (err) {
      // Error is handled by the parent component
    } finally {
      setCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create New Workspace</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit}>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="workspace-name">Workspace Name *</Label>
              <Input
                id="workspace-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Enter workspace name"
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="workspace-description">Description</Label>
              <Textarea
                id="workspace-description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Describe the purpose of this workspace (optional)"
                rows={3}
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={creating}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!name.trim() || creating}>
              {creating ? 'Creating...' : 'Create Workspace'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
