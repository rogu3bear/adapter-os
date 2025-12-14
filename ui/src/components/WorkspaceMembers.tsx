//! Workspace members management component
//!
//! Displays and manages workspace members with role assignment.
//! Allows adding/removing members and changing roles.
//!
//! Citation: Member list table from ui/src/components/Nodes.tsx (Table component)
//! - Table layout with actions
//! - Add/remove member actions
//! - Role management

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
import { useAuth } from '@/providers/CoreProviders';
import { useWorkspaces } from '@/hooks/workspace/useWorkspaces';
import * as types from '@/api/types';
import {
  Users,
  UserPlus,
  UserMinus,
  Crown,
  Shield,
  User,
  AlertCircle,
  RefreshCw
} from 'lucide-react';
import { logger } from '@/utils/logger';

interface WorkspaceMembersProps {
  workspaceId: string;
}

export function WorkspaceMembers({ workspaceId }: WorkspaceMembersProps) {
  const { user } = useAuth();
  const [showAddDialog, setShowAddDialog] = useState(false);

  const {
    listWorkspaceMembers,
    addWorkspaceMember,
    updateWorkspaceMember,
    removeWorkspaceMember,
  } = useWorkspaces({ enabled: false }); // We'll call manually

  const [members, setMembers] = useState<types.WorkspaceMember[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadMembers = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const membersData = await listWorkspaceMembers(workspaceId);
      setMembers(membersData);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to load members';
      setError(errorMessage);
      logger.error('Failed to load workspace members', {
        component: 'WorkspaceMembers',
        operation: 'load_members',
        workspaceId,
      }, err instanceof Error ? err : new Error(String(err)));
    } finally {
      setLoading(false);
    }
  }, [listWorkspaceMembers, workspaceId]);

  React.useEffect(() => {
    loadMembers();
  }, [loadMembers]);

  const handleAddMember = async (data: { user_id: string; role: string }) => {
    try {
      await addWorkspaceMember(workspaceId, {
        user_id: data.user_id,
        role: data.role as 'admin' | 'member' | 'viewer',
        workspace_id: workspaceId,
        tenant_id: user?.tenant_id || '',
      });
      await loadMembers(); // Refresh the list
      setShowAddDialog(false);
      logger.info('Member added to workspace', {
        component: 'WorkspaceMembers',
        operation: 'add_member',
        workspaceId,
        userId: data.user_id,
        role: data.role,
      });
    } catch (err) {
      logger.error('Failed to add workspace member', {
        component: 'WorkspaceMembers',
        operation: 'add_member',
        workspaceId,
        userId: data.user_id,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const handleUpdateRole = async (memberId: string, role: string) => {
    try {
      await updateWorkspaceMember(workspaceId, memberId, role);
      await loadMembers(); // Refresh the list
      logger.info('Member role updated', {
        component: 'WorkspaceMembers',
        operation: 'update_role',
        workspaceId,
        memberId,
        role,
      });
    } catch (err) {
      logger.error('Failed to update member role', {
        component: 'WorkspaceMembers',
        operation: 'update_role',
        workspaceId,
        memberId,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const handleRemoveMember = async (memberId: string) => {
    try {
      await removeWorkspaceMember(workspaceId, memberId);
      await loadMembers(); // Refresh the list
      logger.info('Member removed from workspace', {
        component: 'WorkspaceMembers',
        operation: 'remove_member',
        workspaceId,
        memberId,
      });
    } catch (err) {
      logger.error('Failed to remove workspace member', {
        component: 'WorkspaceMembers',
        operation: 'remove_member',
        workspaceId,
        memberId,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const getRoleIcon = (role: string) => {
    switch (role) {
      case 'owner':
        return Crown;
      case 'admin':
        return Shield;
      case 'member':
        return User;
      default:
        return User;
    }
  };

  const getRoleBadgeVariant = (role: string) => {
    switch (role) {
      case 'owner':
        return 'default';
      case 'admin':
        return 'secondary';
      case 'member':
        return 'outline';
      default:
        return 'outline';
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Users className="h-5 w-5" />
          <h3 className="text-lg font-semibold">Workspace Members</h3>
          <Badge variant="outline">{members.length}</Badge>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={loadMembers}
            disabled={loading}
          >
            <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            onClick={() => setShowAddDialog(true)}
            className="flex items-center gap-2"
          >
            <UserPlus className="h-4 w-4" />
            Add Member
          </Button>
        </div>
      </div>

      {error && (
        <Alert>
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {loading && members.length === 0 ? (
        <div className="space-y-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-12 bg-muted animate-pulse rounded" />
          ))}
        </div>
      ) : members.length === 0 ? (
        <div className="text-center py-8 text-muted-foreground">
          <Users className="h-8 w-8 mx-auto mb-2" />
          <p>No members yet. Add some collaborators to get started!</p>
        </div>
      ) : (
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>User</TableHead>
                <TableHead>Role</TableHead>
                <TableHead>Joined</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {members.map((member) => {
                const RoleIcon = getRoleIcon(member.role);
                return (
                  <TableRow key={member.user_id}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <div className="w-8 h-8 bg-primary/10 rounded-full flex items-center justify-center">
                          <span className="text-sm font-medium">
                            {(member.user_display_name || member.user_id).charAt(0).toUpperCase()}
                          </span>
                        </div>
                        <div>
                          <div className="font-medium">
                            {member.user_display_name || member.user_id}
                          </div>
                          <div className="text-sm text-muted-foreground">
                            {member.user_email || member.user_id}
                          </div>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell>
                      <Select
                        value={member.role}
                        onValueChange={(role) => handleUpdateRole(member.user_id, role)}
                      >
                        <SelectTrigger className="w-32">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="viewer">Viewer</SelectItem>
                          <SelectItem value="member">Member</SelectItem>
                          <SelectItem value="admin">Admin</SelectItem>
                          <SelectItem value="owner">Owner</SelectItem>
                        </SelectContent>
                      </Select>
                    </TableCell>
                    <TableCell>
                      {new Date(member.joined_at).toLocaleDateString()}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => handleRemoveMember(member.user_id)}
                        className="text-destructive hover:text-destructive"
                      >
                        <UserMinus className="h-4 w-4" />
                      </Button>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </Card>
      )}

      {/* Add Member Dialog */}
      <AddMemberDialog
        open={showAddDialog}
        onOpenChange={setShowAddDialog}
        onAdd={handleAddMember}
      />
    </div>
  );
}

// Add Member Dialog
interface AddMemberDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdd: (data: { user_id: string; role: string }) => Promise<void>;
}

function AddMemberDialog({ open, onOpenChange, onAdd }: AddMemberDialogProps) {
  const [userId, setUserId] = useState('');
  const [role, setRole] = useState('member');
  const [adding, setAdding] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!userId.trim()) return;

    setAdding(true);
    try {
      await onAdd({ user_id: userId.trim(), role });
      setUserId('');
      setRole('member');
    } catch (err) {
      // Error handled by parent
    } finally {
      setAdding(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add Workspace Member</DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit}>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="user-id">User ID</Label>
              <Input
                id="user-id"
                value={userId}
                onChange={(e) => setUserId(e.target.value)}
                placeholder="Enter user ID or email"
                required
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="user-role">Role</Label>
              <Select value={role} onValueChange={setRole}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="viewer">Viewer</SelectItem>
                  <SelectItem value="member">Member</SelectItem>
                  <SelectItem value="admin">Admin</SelectItem>
                  <SelectItem value="owner">Owner</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={adding}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!userId.trim() || adding}>
              {adding ? 'Adding...' : 'Add Member'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
