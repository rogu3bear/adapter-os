import { useState } from 'react';
import { DataTable } from '@/components/shared/DataTable/DataTable';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  MoreHorizontal,
  Edit,
  Trash2,
  Shield,
  KeyRound,
  UserCheck,
  UserX,
} from 'lucide-react';
import type { User, UserRole } from '@/api/types';
import type { ColumnDef } from '@/components/shared/DataTable/types';
import { UserFormModal } from './UserFormModal';
import {
  useDeleteUser,
  useResetUserPassword,
  useSetUserActive,
} from '@/hooks/admin/useAdmin';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';

interface UserTableProps {
  users: User[];
}

const roleVariants: Record<UserRole, 'default' | 'secondary' | 'outline' | 'destructive'> = {
  admin: 'destructive',
  operator: 'default',
  sre: 'secondary',
  compliance: 'outline',
  auditor: 'outline',
  viewer: 'outline',
  developer: 'secondary',
};

const roleLabels: Record<UserRole, string> = {
  admin: 'Admin',
  operator: 'Operator',
  sre: 'SRE',
  compliance: 'Compliance',
  auditor: 'Auditor',
  viewer: 'Viewer',
  developer: 'Developer',
};

export function UserTable({ users }: UserTableProps) {
  const [editingUser, setEditingUser] = useState<User | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<User | null>(null);
  const [confirmResetPassword, setConfirmResetPassword] = useState<User | null>(null);

  const deleteUser = useDeleteUser();
  const resetPassword = useResetUserPassword();
  const setUserActive = useSetUserActive();

  const handleDelete = async () => {
    if (confirmDelete) {
      const userId = confirmDelete.user_id || confirmDelete.id;
      if (userId) {
        await deleteUser.mutateAsync(userId);
      }
      setConfirmDelete(null);
    }
  };

  const handleResetPassword = async () => {
    if (confirmResetPassword) {
      const userId = confirmResetPassword.user_id || confirmResetPassword.id;
      if (userId) {
        await resetPassword.mutateAsync(userId);
      }
      setConfirmResetPassword(null);
    }
  };

  const handleToggleActive = async (user: User) => {
    const userId = user.user_id || user.id;
    if (userId) {
      // Determine current active state (default to true if not specified)
      const isCurrentlyActive = true; // Users are active by default
      await setUserActive.mutateAsync({ userId, isActive: !isCurrentlyActive });
    }
  };

  const columns: ColumnDef<User>[] = [
    {
      id: 'email',
      header: 'Email',
      accessorKey: 'email',
      enableSorting: true,
      cell: (context) => (
        <span className="font-medium">{context.getValue() as string}</span>
      ),
    },
    {
      id: 'display_name',
      header: 'Display Name',
      accessorKey: 'display_name',
      enableSorting: true,
      cell: (context) => {
        const name = context.getValue() as string | undefined;
        return name ? (
          <span>{name}</span>
        ) : (
          <span className="text-muted-foreground italic">Not set</span>
        );
      },
    },
    {
      id: 'role',
      header: 'Role',
      accessorKey: 'role',
      enableSorting: true,
      cell: (context) => {
        const role = context.getValue() as UserRole;
        return (
          <Badge variant={roleVariants[role] || 'outline'}>
            <Shield className="h-3 w-3 mr-1" />
            {roleLabels[role] || role}
          </Badge>
        );
      },
    },
    {
      id: 'tenant_id',
      header: 'Workspace',
      accessorKey: 'tenant_id',
      cell: (context) => {
        const tenantId = context.getValue() as string | undefined;
        return tenantId ? (
          <span className="font-mono text-xs">{tenantId}</span>
        ) : (
          <span className="text-muted-foreground">Global</span>
        );
      },
    },
    {
      id: 'mfa_enabled',
      header: 'MFA',
      accessorKey: 'mfa_enabled',
      cell: (context) => {
        const mfaEnabled = context.getValue() as boolean | undefined;
        return mfaEnabled ? (
          <Badge variant="default">Enabled</Badge>
        ) : (
          <Badge variant="outline">Disabled</Badge>
        );
      },
    },
    {
      id: 'last_login',
      header: 'Last Login',
      accessorFn: (row) => row.last_login || row.last_login_at,
      enableSorting: true,
      cell: (context) => {
        const date = context.getValue() as string | undefined;
        return date ? (
          <span className="text-sm">{new Date(date).toLocaleString()}</span>
        ) : (
          <span className="text-muted-foreground">Never</span>
        );
      },
    },
    {
      id: 'created_at',
      header: 'Created',
      accessorKey: 'created_at',
      enableSorting: true,
      cell: (context) => {
        const date = context.getValue() as string | undefined;
        return date ? new Date(date).toLocaleDateString() : 'N/A';
      },
    },
    {
      id: 'actions',
      header: 'Actions',
      cell: (context) => {
        const user = context.row.original;
        return (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm">
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => setEditingUser(user)}>
                <Edit className="h-4 w-4 mr-2" />
                Edit User
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setConfirmResetPassword(user)}>
                <KeyRound className="h-4 w-4 mr-2" />
                Reset Password
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => handleToggleActive(user)}>
                <UserCheck className="h-4 w-4 mr-2" />
                Toggle Active
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={() => setConfirmDelete(user)}
                className="text-destructive"
              >
                <Trash2 className="h-4 w-4 mr-2" />
                Delete User
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        );
      },
    },
  ];

  return (
    <>
      <DataTable
        data={users}
        columns={columns}
        getRowId={(row) => row.user_id || row.id || row.email}
        enableSorting
        enablePagination
        emptyStateMessage="No users found"
      />

      {editingUser && (
        <UserFormModal
          open={!!editingUser}
          onOpenChange={(open) => !open && setEditingUser(null)}
          user={editingUser}
        />
      )}

      {/* Delete Confirmation Dialog */}
      <Dialog open={!!confirmDelete} onOpenChange={() => setConfirmDelete(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete User</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete the user "{confirmDelete?.email}"? This action cannot
              be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDelete(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleteUser.isPending}
            >
              {deleteUser.isPending ? 'Deleting...' : 'Delete'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Reset Password Confirmation Dialog */}
      <Dialog open={!!confirmResetPassword} onOpenChange={() => setConfirmResetPassword(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Reset Password</DialogTitle>
            <DialogDescription>
              Are you sure you want to send a password reset email to "{confirmResetPassword?.email}"?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmResetPassword(null)}>
              Cancel
            </Button>
            <Button
              onClick={handleResetPassword}
              disabled={resetPassword.isPending}
            >
              {resetPassword.isPending ? 'Sending...' : 'Send Reset Email'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
