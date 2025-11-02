//! Workspace resources management component
//!
//! Displays and manages shared resources in a workspace.
//! Allows sharing and unsharing resources.
//!
//! Citation: Resource list from adapter/node display patterns
//! - Table layout for resources
//! - Share/unshare resource actions

import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useWorkspaces } from '@/hooks/useWorkspaces';
import {
  FolderOpen,
  Share,
  X,
  AlertCircle,
  RefreshCw,
  Box,
  Cpu,
  FileText,
  GitBranch,
  Settings
} from 'lucide-react';
import { logger } from '@/utils/logger';

interface WorkspaceResourcesProps {
  workspaceId: string;
}

export function WorkspaceResources({ workspaceId }: WorkspaceResourcesProps) {
  const [showShareDialog, setShowShareDialog] = useState(false);

  const {
    listWorkspaceResources,
    shareWorkspaceResource,
    unshareWorkspaceResource,
  } = useWorkspaces({ enabled: false }); // We'll call manually

  const [resources, setResources] = useState<any[]>([]); // TODO: Use proper type
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadResources = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const resourcesData = await listWorkspaceResources(workspaceId);
      setResources(resourcesData);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load resources';
      setError(errorMessage);
      logger.error('Failed to load workspace resources', {
        component: 'WorkspaceResources',
        operation: 'load_resources',
        workspaceId,
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      setLoading(false);
    }
  }, [listWorkspaceResources, workspaceId]);

  React.useEffect(() => {
    loadResources();
  }, [loadResources]);

  const handleShareResource = async (data: { resource_type: string; resource_id: string }) => {
    try {
      await shareWorkspaceResource(workspaceId, data.resource_type, data.resource_id);
      await loadResources(); // Refresh the list
      setShowShareDialog(false);
      logger.info('Resource shared in workspace', {
        component: 'WorkspaceResources',
        operation: 'share_resource',
        workspaceId,
        resourceType: data.resource_type,
        resourceId: data.resource_id,
      });
    } catch (err) {
      logger.error('Failed to share workspace resource', {
        component: 'WorkspaceResources',
        operation: 'share_resource',
        workspaceId,
        resourceType: data.resource_type,
        resourceId: data.resource_id,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const handleUnshareResource = async (resourceId: string, resourceType: string) => {
    try {
      await unshareWorkspaceResource(workspaceId, resourceId, resourceType);
      await loadResources(); // Refresh the list
      logger.info('Resource unshared from workspace', {
        component: 'WorkspaceResources',
        operation: 'unshare_resource',
        workspaceId,
        resourceType,
        resourceId,
      });
    } catch (err) {
      logger.error('Failed to unshare workspace resource', {
        component: 'WorkspaceResources',
        operation: 'unshare_resource',
        workspaceId,
        resourceType,
        resourceId,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const getResourceTypeIcon = (type: string) => {
    switch (type) {
      case 'adapter':
        return Box;
      case 'model':
        return Cpu;
      case 'policy':
        return Settings;
      case 'repository':
        return GitBranch;
      case 'plan':
        return FileText;
      default:
        return FolderOpen;
    }
  };

  const getResourceTypeBadgeVariant = (type: string) => {
    switch (type) {
      case 'adapter':
        return 'default';
      case 'model':
        return 'secondary';
      case 'policy':
        return 'destructive';
      case 'repository':
        return 'outline';
      case 'plan':
        return 'outline';
      default:
        return 'outline';
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <FolderOpen className="h-5 w-5" />
          <h3 className="text-lg font-semibold">Shared Resources</h3>
          <Badge variant="outline">{resources.length}</Badge>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={loadResources}
            disabled={loading}
          >
            <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            onClick={() => setShowShareDialog(true)}
            className="flex items-center gap-2"
          >
            <Share className="h-4 w-4" />
            Share Resource
          </Button>
        </div>
      </div>

      {error && (
        <Alert>
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {loading && resources.length === 0 ? (
        <div className="space-y-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-12 bg-muted animate-pulse rounded" />
          ))}
        </div>
      ) : resources.length === 0 ? (
        <div className="text-center py-8 text-muted-foreground">
          <FolderOpen className="h-8 w-8 mx-auto mb-2" />
          <p>No shared resources yet. Share adapters, models, or other resources to collaborate!</p>
        </div>
      ) : (
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Resource</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Shared By</TableHead>
                <TableHead>Shared Date</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {resources.map((resource) => {
                const TypeIcon = getResourceTypeIcon(resource.resource_type);
                return (
                  <TableRow key={`${resource.resource_type}-${resource.resource_id}`}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <TypeIcon className="h-4 w-4 text-muted-foreground" />
                        <div>
                          <div className="font-medium">
                            {resource.resource_name || `${resource.resource_type} ${resource.resource_id.slice(0, 8)}`}
                          </div>
                          <div className="text-sm text-muted-foreground">
                            ID: {resource.resource_id}
                          </div>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant={getResourceTypeBadgeVariant(resource.resource_type)}>
                        {resource.resource_type}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      {resource.shared_by}
                    </TableCell>
                    <TableCell>
                      {new Date(resource.shared_at).toLocaleDateString()}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => handleUnshareResource(resource.resource_id, resource.resource_type)}
                        className="text-destructive hover:text-destructive"
                      >
                        <X className="h-4 w-4" />
                      </Button>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </Card>
      )}

      {/* Share Resource Dialog */}
      <ShareResourceDialog
        open={showShareDialog}
        onOpenChange={setShowShareDialog}
        onShare={handleShareResource}
      />
    </div>
  );
}

// Share Resource Dialog
interface ShareResourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onShare: (data: { resource_type: string; resource_id: string }) => Promise<void>;
}

function ShareResourceDialog({ open, onOpenChange, onShare }: ShareResourceDialogProps) {
  const [resourceType, setResourceType] = useState('adapter');
  const [resourceId, setResourceId] = useState('');
  const [sharing, setSharing] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!resourceId.trim()) return;

    setSharing(true);
    try {
      await onShare({ resource_type: resourceType, resource_id: resourceId.trim() });
      setResourceId('');
      setResourceType('adapter');
    } catch (err) {
      // Error handled by parent
    } finally {
      setSharing(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Share Resource in Workspace</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit}>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="resource-type">Resource Type</Label>
              <Select value={resourceType} onValueChange={setResourceType}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="adapter">Adapter</SelectItem>
                  <SelectItem value="model">Model</SelectItem>
                  <SelectItem value="policy">Policy</SelectItem>
                  <SelectItem value="repository">Repository</SelectItem>
                  <SelectItem value="plan">Plan</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="resource-id">Resource ID</Label>
              <Input
                id="resource-id"
                value={resourceId}
                onChange={(e) => setResourceId(e.target.value)}
                placeholder="Enter resource ID"
                required
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={sharing}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!resourceId.trim() || sharing}>
              {sharing ? 'Sharing...' : 'Share Resource'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
