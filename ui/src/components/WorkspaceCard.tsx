//! Workspace card component
//!
//! Displays workspace summary with quick actions.
//! Shows member count, resource count, and last activity.
//!
//! Citation: Card display from ui/src/components/dashboard/ActivityFeedWidget.tsx L85-L196
//! - Card layout with header, content, and actions
//! - Badge patterns from ui/src/components/ui/badge.tsx

import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Workspace } from '@/api/types';
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

interface WorkspaceCardProps {
  workspace: Workspace;
  onSelect: (workspaceId: string) => void;
  onEdit: (workspaceId: string, data: { name?: string; description?: string }) => Promise<Workspace>;
  onDelete: (workspaceId: string) => Promise<void>;
}

export function WorkspaceCard({ workspace, onSelect, onEdit, onDelete }: WorkspaceCardProps) {
  const [showEditDialog, setShowEditDialog] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);

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

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    });
  };

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
                  <span>0 members</span> {/* TODO: Add member count */}
                </div>
                <div className="flex items-center gap-1 text-muted-foreground">
                  <FolderOpen className="h-4 w-4" />
                  <span>0 resources</span> {/* TODO: Add resource count */}
                </div>
              </div>
            </div>

            {/* Activity */}
            <div className="flex items-center gap-1 text-xs text-muted-foreground">
              <MessageSquare className="h-3 w-3" />
              <span>Last active: Never</span> {/* TODO: Add last activity */}
            </div>

            {/* Created date */}
            <div className="flex items-center gap-1 text-xs text-muted-foreground">
              <Calendar className="h-3 w-3" />
              <span>Created {formatDate(workspace.created_at)}</span>
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
