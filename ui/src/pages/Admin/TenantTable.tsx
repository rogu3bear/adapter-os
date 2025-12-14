import { useState } from 'react';
import { DataTable } from '@/components/shared/DataTable/DataTable';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { MoreHorizontal, Edit, Pause, Archive, BarChart3 } from 'lucide-react';
import type { Tenant } from '@/api/types';
import type { ColumnDef } from '@/components/shared/DataTable/types';
import { TenantFormModal } from './TenantFormModal';
import { TenantDetailPage } from './TenantDetailPage';
import { usePauseTenant, useArchiveTenant } from '@/hooks/admin/useAdmin';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';

interface TenantTableProps {
  tenants: Tenant[];
}

export function TenantTable({ tenants }: TenantTableProps) {
  const [editingTenant, setEditingTenant] = useState<Tenant | null>(null);
  const [viewingTenant, setViewingTenant] = useState<Tenant | null>(null);
  const [confirmArchive, setConfirmArchive] = useState<Tenant | null>(null);
  const pauseTenant = usePauseTenant();
  const archiveTenant = useArchiveTenant();

  const handlePause = async (tenant: Tenant) => {
    await pauseTenant.mutateAsync(tenant.id);
  };

  const handleArchive = async () => {
    if (confirmArchive) {
      await archiveTenant.mutateAsync(confirmArchive.id);
      setConfirmArchive(null);
    }
  };

  const columns: ColumnDef<Tenant>[] = [
    {
      id: 'tenant_id',
      header: 'Organization ID',
      accessorKey: 'id',
      enableSorting: true,
      cell: (context) => (
        <span className="font-mono text-sm">{context.getValue() as string}</span>
      ),
    },
    {
      id: 'name',
      header: 'Name',
      accessorKey: 'name',
      enableSorting: true,
      cell: (context) => (
        <span className="font-medium">{context.getValue() as string}</span>
      ),
    },
    {
      id: 'status',
      header: 'Status',
      accessorKey: 'status',
      enableSorting: true,
      cell: (context) => {
        const status = context.getValue() as string | undefined;
        const variant =
          status === 'active'
            ? 'default'
            : status === 'paused'
            ? 'secondary'
            : 'outline';
        return (
          <Badge variant={variant}>
            {status || 'active'}
          </Badge>
        );
      },
    },
    {
      id: 'isolation_level',
      header: 'Isolation',
      accessorKey: 'isolation_level',
      cell: (context) => {
        const level = context.getValue() as string | undefined;
        return (
          <Badge variant="outline">
            {level || 'standard'}
          </Badge>
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
        const tenant = context.row.original;
        return (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm">
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => setViewingTenant(tenant)}>
                <BarChart3 className="h-4 w-4 mr-2" />
                View Details
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setEditingTenant(tenant)}>
                <Edit className="h-4 w-4 mr-2" />
                Edit
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => handlePause(tenant)}>
                <Pause className="h-4 w-4 mr-2" />
                Pause
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => setConfirmArchive(tenant)}
                className="text-destructive"
              >
                <Archive className="h-4 w-4 mr-2" />
                Archive
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
        data={tenants}
        columns={columns}
        getRowId={(row) => row.id}
        enableSorting
        enablePagination
        emptyStateMessage="No organizations found"
      />

      {editingTenant && (
        <TenantFormModal
          open={!!editingTenant}
          onOpenChange={(open) => !open && setEditingTenant(null)}
          tenant={editingTenant}
        />
      )}

      {viewingTenant && (
        <TenantDetailPage
          tenant={viewingTenant}
          open={!!viewingTenant}
          onClose={() => setViewingTenant(null)}
        />
      )}

      <Dialog open={!!confirmArchive} onOpenChange={() => setConfirmArchive(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Archive Organization</DialogTitle>
            <DialogDescription>
              Are you sure you want to archive organization "{confirmArchive?.name}"? This action can be
              undone later.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmArchive(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleArchive}>
              Archive
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
