//! Workspace card component
//!
//! Displays workspace summary with quick actions.
//! Shows member count, resource count, and last activity.
//!
//! Citation: Card display from ui/src/components/dashboard/ActivityFeedWidget.tsx L85-L196
//! - Card layout with header, content, and actions
//! - Badge patterns from ui/src/components/ui/badge.tsx

import React, { useState, useEffect, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Workspace } from '@/api/types';
import { useWorkspaces } from '@/hooks/workspace/useWorkspaces';
import apiClient from '@/api/client';
import {
  Building,
  Users,
  FolderOpen,
  Edit3,
  Trash2,
  MoreHorizontal,
  MessageSquare,
  Calendar
} from 'lucide-react';
import { logger } from '@/utils/logger';
import { formatTimestamp } from '@/utils/format';

interface WorkspaceCardProps {
  workspace: Workspace;
  onSelect: (workspaceId: string) => void;
  onEdit: (workspaceId: string, data: { name?: string; description?: string }) => Promise<Workspace>;
  onDelete: (workspaceId: string) => Promise<void>;
}

export function WorkspaceCard({ workspace, onSelect, onEdit, onDelete }: WorkspaceCardProps) {
  const [showEditDialog, setShowEditDialog] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [memberCount, setMemberCount] = useState<number | null>(null);
  const [resourceCount, setResourceCount] = useState<number | null>(null);
  const [lastActivity, setLastActivity] = useState<string | null>(null);
  const { listWorkspaceMembers, listWorkspaceResources } = useWorkspaces({ enabled: false });
  
  // Cache to avoid refetching on every render
  const cacheRef = useRef<{
    workspaceId: string;
    memberCount: number | null;
    resourceCount: number | null;
    lastActivity: string | null;
    timestamp: number;
  } | null>(null);
  
  const CACHE_TTL_MS = 30000; // 30 seconds cache

  const handleEdit = async (data: { name?: string; description?: string }) => {
    try {
      await onEdit(workspace.id, data);
      setShowEditDialog(false);
      logger.info('Workspace edited from card', {
        component: 'WorkspaceCard',
        operation: 'edit_workspace',
        workspaceId: workspace.id,
      });
    } catch (err) {
      logger.error('Failed to edit workspace from card', {
        component: 'WorkspaceCard',
        operation: 'edit_workspace',
        workspaceId: workspace.id,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const handleDelete = async () => {
    try {
      await onDelete(workspace.id);
      setShowDeleteDialog(false);
      logger.info('Workspace deleted from card', {
        component: 'WorkspaceCard',
        operation: 'delete_workspace',
        workspaceId: workspace.id,
      });
    } catch (err) {
      logger.error('Failed to delete workspace from card', {
        component: 'WorkspaceCard',
        operation: 'delete_workspace',
        workspaceId: workspace.id,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  useEffect(() => {
    // Check cache first
    const now = Date.now();
    if (
      cacheRef.current &&
      cacheRef.current.workspaceId === workspace.id &&
      (now - cacheRef.current.timestamp) < CACHE_TTL_MS
    ) {
      setMemberCount(cacheRef.current.memberCount);
      setResourceCount(cacheRef.current.resourceCount);
      setLastActivity(cacheRef.current.lastActivity);
      return;
    }
    
    let cancelled = false;
    let timeoutId: NodeJS.Timeout | null = null;
    
    // Debounce: wait 300ms before fetching to avoid rapid refetches
    timeoutId = setTimeout(() => {
      const loadCounts = async () => {
        try {
          const [members, resources, activityEvents] = await Promise.all([
            listWorkspaceMembers(workspace.id).catch(() => []),
            listWorkspaceResources(workspace.id).catch(() => []),
            apiClient.listActivityEvents({ workspace_id: workspace.id, limit: 1 }).catch(() => []),
          ]);
          
          if (cancelled) return;
          
          const memberCountValue = members.length;
          const resourceCountValue = resources.length;
          
          // Get last activity from most recent activity event, member addition, or resource share
          const memberDates = members.map(m => new Date(m.joined_at).getTime());
          const resourceDates = resources.map(r => r.shared_at ? new Date(r.shared_at).getTime() : 0).filter(d => d > 0);
          const activityDates = activityEvents.map(e => e.created_at ? new Date(e.created_at).getTime() : 0).filter(d => d > 0);
          const allDates = [...memberDates, ...resourceDates, ...activityDates];
          
          let lastActivityValue: string | null = null;
          if (allDates.length > 0) {
            const latestDate = new Date(Math.max(...allDates));
            const diffMs = now - latestDate.getTime();
            const diffMins = Math.floor(diffMs / 60000);
            const diffHours = Math.floor(diffMins / 60);
            const diffDays = Math.floor(diffHours / 24);
            
            if (diffMins < 1) {
              lastActivityValue = 'Just now';
            } else if (diffMins < 60) {
              lastActivityValue = `${diffMins}m ago`;
            } else if (diffHours < 24) {
              lastActivityValue = `${diffHours}h ago`;
            } else if (diffDays < 7) {
              lastActivityValue = `${diffDays}d ago`;
            } else {
              lastActivityValue = latestDate.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
            }
          }
          
          // Update cache
          cacheRef.current = {
            workspaceId: workspace.id,
            memberCount: memberCountValue,
            resourceCount: resourceCountValue,
            lastActivity: lastActivityValue,
            timestamp: now,
          };
          
          if (!cancelled) {
            setMemberCount(memberCountValue);
            setResourceCount(resourceCountValue);
            setLastActivity(lastActivityValue);
          }
        } catch (err) {
          if (cancelled) return;
          logger.error('Failed to load workspace counts', {
            component: 'WorkspaceCard',
            operation: 'loadCounts',
            workspaceId: workspace.id,
          }, err instanceof Error ? err : new Error(String(err)));
        }
      };
      
      loadCounts();
    }, 300);
    
    return () => {
      cancelled = true;
      if (timeoutId) clearTimeout(timeoutId);
    };
  }, [workspace.id, listWorkspaceMembers, listWorkspaceResources]);

  return (
    <>
      <Card className="cursor-pointer hover:shadow-md transition-shadow" onClick={() => onSelect(workspace.id)}>
        <CardHeader className="pb-3">
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-2">
              <Building className="h-5 w-5 text-primary" />
              <CardTitle className="text-lg truncate">{workspace.name}</CardTitle>
            </div>
            <div className="flex items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                onClick={(e) => {
                  e.stopPropagation();
                  setShowEditDialog(true);
                }}
                aria-label="Edit workspace"
              >
                <Edit3 className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={(e) => {
                  e.stopPropagation();
                  setShowDeleteDialog(true);
                }}
                aria-label="Delete workspace"
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
          </div>
          {workspace.description && (
            <p className="text-sm text-muted-foreground line-clamp-2">
              {workspace.description}
            </p>
          )}
        </CardHeader>

        <CardContent className="pt-0">
          <div className="space-y-3">
            {/* Stats */}
            <div className="flex items-center justify-between text-sm">
              <div className="flex items-center gap-4">
                <div className="flex items-center gap-1 text-muted-foreground">
                  <Users className="h-4 w-4" />
                  <span>{memberCount !== null ? `${memberCount} ${memberCount === 1 ? 'member' : 'members'}` : '...'}</span>
                </div>
                <div className="flex items-center gap-1 text-muted-foreground">
                  <FolderOpen className="h-4 w-4" />
                  <span>{resourceCount !== null ? `${resourceCount} ${resourceCount === 1 ? 'resource' : 'resources'}` : '...'}</span>
                </div>
              </div>
            </div>

            {/* Activity */}
            <div className="flex items-center gap-1 text-xs text-muted-foreground">
              <MessageSquare className="h-3 w-3" />
              <span>Last active: {lastActivity || 'Never'}</span>
            </div>

            {/* Created date */}
            <div className="flex items-center gap-1 text-xs text-muted-foreground">
              <Calendar className="h-3 w-3" />
              <span>Created {formatTimestamp(workspace.created_at, 'long')}</span>
            </div>

            {/* Quick actions */}
            <div className="flex gap-2 pt-2">
              <Button
                variant="outline"
                size="sm"
                className="flex-1"
                onClick={(e) => {
                  e.stopPropagation();
                  onSelect(workspace.id);
                }}
              >
                View Details
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Edit Dialog */}
      <EditWorkspaceDialog
        workspace={workspace}
        open={showEditDialog}
        onOpenChange={setShowEditDialog}
        onEdit={handleEdit}
      />

      {/* Delete Dialog */}
      <DeleteWorkspaceDialog
        workspace={workspace}
        open={showDeleteDialog}
        onOpenChange={setShowDeleteDialog}
        onDelete={handleDelete}
      />
    </>
  );
}

// Edit Workspace Dialog
interface EditWorkspaceDialogProps {
  workspace: Workspace;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onEdit: (data: { name?: string; description?: string }) => Promise<void>;
}

function EditWorkspaceDialog({ workspace, open, onOpenChange, onEdit }: EditWorkspaceDialogProps) {
  const [name, setName] = useState(workspace.name);
  const [description, setDescription] = useState(workspace.description || '');
  const [editing, setEditing] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    setEditing(true);
    try {
      await onEdit({
        name: name.trim(),
        description: description.trim() || undefined,
      });
    } catch (err) {
      // Error handled by parent
    } finally {
      setEditing(false);
    }
  };

  // Reset form when dialog opens
  React.useEffect(() => {
    if (open) {
      setName(workspace.name);
      setDescription(workspace.description || '');
    }
  }, [open, workspace]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit Workspace</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit}>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="edit-workspace-name">Workspace Name</Label>
              <Input
                id="edit-workspace-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Enter workspace name"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="edit-workspace-description">Description</Label>
              <Textarea
                id="edit-workspace-description"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Describe the purpose of this workspace"
                rows={3}
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={editing}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={editing}>
              {editing ? 'Saving...' : 'Save Changes'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// Delete Workspace Dialog
interface DeleteWorkspaceDialogProps {
  workspace: Workspace;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onDelete: () => Promise<void>;
}

function DeleteWorkspaceDialog({ workspace, open, onOpenChange, onDelete }: DeleteWorkspaceDialogProps) {
  const [deleting, setDeleting] = useState(false);

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await onDelete();
    } catch (err) {
      // Error handled by parent
    } finally {
      setDeleting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete Workspace</DialogTitle>
        </DialogHeader>
        <div className="py-4">
          <p className="text-sm text-muted-foreground">
            Are you sure you want to delete the workspace <strong>{workspace.name}</strong>?
            This action cannot be undone and will remove all associated messages and resources.
          </p>
        </div>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={deleting}
          >
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={handleDelete}
            disabled={deleting}
          >
            {deleting ? 'Deleting...' : 'Delete Workspace'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
