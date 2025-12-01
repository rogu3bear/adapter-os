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
import { MoreHorizontal, Edit, Trash2, Play, Square, Eye, Star, StarOff } from 'lucide-react';
import type { AdapterStack, PolicyPreflightResponse } from '@/api/types';
import type { ColumnDef } from '@/components/shared/DataTable/types';
import { StackFormModal } from './StackFormModal';
import { StackDetailModal } from './StackDetailModal';
import {
  useDeleteAdapterStack,
  useActivateAdapterStack,
  useDeactivateAdapterStack,
  useSetDefaultStack,
  useGetDefaultStack,
  useClearDefaultStack,
} from '@/hooks/useAdmin';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { useTenant } from '@/layout/LayoutProvider';
import { PolicyPreflightDialog } from '@/components/PolicyPreflightDialog';
import apiClient from '@/api/client';
import { toast } from 'sonner';

interface StackTableProps {
  stacks: AdapterStack[];
}

export function StackTable({ stacks }: StackTableProps) {
  const { selectedTenant } = useTenant();
  const tenantId = selectedTenant || 'default';
  const [editingStack, setEditingStack] = useState<AdapterStack | null>(null);
  const [viewingStack, setViewingStack] = useState<AdapterStack | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<AdapterStack | null>(null);
  const [showPreflightDialog, setShowPreflightDialog] = useState(false);
  const [preflightData, setPreflightData] = useState<PolicyPreflightResponse | null>(null);
  const [pendingActivation, setPendingActivation] = useState<AdapterStack | null>(null);
  const deleteStack = useDeleteAdapterStack();
  const activateStack = useActivateAdapterStack();
  const deactivateStack = useDeactivateAdapterStack();
  const setDefaultStack = useSetDefaultStack(tenantId);
  const clearDefaultStack = useClearDefaultStack(tenantId);
  const { data: defaultStack } = useGetDefaultStack(tenantId);

  const handleActivate = async (stack: AdapterStack) => {
    try {
      // Run preflight policy checks
      const preflight = await apiClient.preflightStackActivation(stack.id);
      setPreflightData(preflight);
      setPendingActivation(stack);

      if (!preflight.can_proceed || preflight.checks.some(c => !c.passed)) {
        // Show preflight dialog if there are concerns
        setShowPreflightDialog(true);
      } else {
        // All checks passed, proceed with activation
        await activateStack.mutateAsync(stack.id);
      }
    } catch (error) {
      toast.error(`Failed to run preflight checks: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  };

  const doActivateStack = async () => {
    if (!pendingActivation) return;

    try {
      await activateStack.mutateAsync(pendingActivation.id);
      setShowPreflightDialog(false);
      setPendingActivation(null);
    } catch (error) {
      toast.error(`Failed to activate stack: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  };

  const handleDeactivate = async () => {
    await deactivateStack.mutateAsync();
  };

  const handleDelete = async () => {
    if (confirmDelete) {
      await deleteStack.mutateAsync(confirmDelete.id);
      setConfirmDelete(null);
    }
  };

  const handleSetDefault = async (stack: AdapterStack) => {
    await setDefaultStack.mutateAsync(stack.id);
  };

  const handleClearDefault = async () => {
    await clearDefaultStack.mutateAsync();
  };

  const isDefaultStack = (stackId: string) => {
    return defaultStack?.id === stackId;
  };

  const columns: ColumnDef<AdapterStack>[] = [
    {
      id: 'name',
      header: 'Name',
      accessorKey: 'name',
      enableSorting: true,
      cell: (context) => {
        const stack = context.row.original;
        const isDefault = isDefaultStack(stack.id);
        return (
          <div className="flex items-center gap-2">
            <span className="font-medium">{context.getValue() as string}</span>
            {isDefault && (
              <Badge variant="default" className="text-xs">
                <Star className="h-3 w-3 mr-1" />
                Default
              </Badge>
            )}
          </div>
        );
      },
    },
    {
      id: 'adapters',
      header: 'Adapters',
      accessorKey: 'adapters',
      cell: (context) => {
        const adapters = context.getValue() as Array<{ adapter_id?: string } | string>;
        return (
          <div className="flex flex-wrap gap-1">
            {adapters.slice(0, 3).map((adapter, idx) => {
              const adapterId = typeof adapter === 'object' && adapter !== null && 'adapter_id' in adapter ? adapter.adapter_id : adapter;
              return (
                <Badge key={idx} variant="outline" className="text-xs">
                  {String(adapterId)}
                </Badge>
              );
            })}
            {adapters.length > 3 && (
              <Badge variant="secondary" className="text-xs">
                +{adapters.length - 3} more
              </Badge>
            )}
          </div>
        );
      },
    },
    {
      id: 'description',
      header: 'Description',
      accessorKey: 'description',
      cell: (context) => {
        const desc = context.getValue() as string | undefined;
        return (
          <span className="text-sm text-muted-foreground truncate max-w-xs">
            {desc || 'No description'}
          </span>
        );
      },
    },
    {
      id: 'lifecycle_state',
      header: 'State',
      accessorKey: 'lifecycle_state',
      enableSorting: true,
      cell: (context) => {
        const state = (context.getValue() as string | undefined) || 'active';
        const stateConfig: Record<string, { variant: 'default' | 'secondary' | 'outline'; className: string }> = {
          active: { variant: 'default', className: 'bg-success text-white hover:bg-success/90' },
          deprecated: { variant: 'secondary', className: 'bg-warning text-white hover:bg-warning/90' },
          retired: { variant: 'outline', className: 'bg-muted text-white hover:bg-muted/90' },
          draft: { variant: 'secondary', className: 'bg-info text-white hover:bg-info/90' },
        };
        const config = stateConfig[state.toLowerCase()] || stateConfig.active;
        return (
          <Badge variant={config.variant} className={`text-xs ${config.className}`}>
            {state.charAt(0).toUpperCase() + state.slice(1)}
          </Badge>
        );
      },
    },
    {
      id: 'version',
      header: 'Version',
      accessorKey: 'version',
      enableSorting: true,
      cell: (context) => {
        const version = context.getValue() as number | undefined;
        return (
          <span className="text-sm font-mono">
            {version ?? 1}
          </span>
        );
      },
    },
    {
      id: 'created_at',
      header: 'Created',
      accessorKey: 'created_at',
      enableSorting: true,
      cell: (context) => {
        const date = context.getValue() as string;
        return new Date(date).toLocaleDateString();
      },
    },
    {
      id: 'actions',
      header: 'Actions',
      cell: (context) => {
        const stack = context.row.original;
        return (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm">
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => setViewingStack(stack)}>
                <Eye className="h-4 w-4 mr-2" />
                View Details
              </DropdownMenuItem>
              {isDefaultStack(stack.id) ? (
                <DropdownMenuItem onClick={handleClearDefault}>
                  <StarOff className="h-4 w-4 mr-2" />
                  Clear Default
                </DropdownMenuItem>
              ) : (
                <DropdownMenuItem onClick={() => handleSetDefault(stack)}>
                  <Star className="h-4 w-4 mr-2" />
                  Set as Default
                </DropdownMenuItem>
              )}
              <DropdownMenuItem onClick={() => handleActivate(stack)}>
                <Play className="h-4 w-4 mr-2" />
                Activate
              </DropdownMenuItem>
              <DropdownMenuItem onClick={handleDeactivate}>
                <Square className="h-4 w-4 mr-2" />
                Deactivate
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setEditingStack(stack)}>
                <Edit className="h-4 w-4 mr-2" />
                Edit
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => setConfirmDelete(stack)}
                className="text-destructive"
              >
                <Trash2 className="h-4 w-4 mr-2" />
                Delete
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
        data={stacks}
        columns={columns}
        getRowId={(row) => row.id}
        enableSorting
        enablePagination
        emptyStateMessage="No adapter stacks found"
      />

      {editingStack && (
        <StackFormModal
          open={!!editingStack}
          onOpenChange={(open) => !open && setEditingStack(null)}
          stack={editingStack}
        />
      )}

      {viewingStack && (
        <StackDetailModal
          stack={viewingStack}
          open={!!viewingStack}
          onClose={() => setViewingStack(null)}
        />
      )}

      <Dialog open={!!confirmDelete} onOpenChange={() => setConfirmDelete(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Adapter Stack</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete adapter stack "{confirmDelete?.name}"? This action
              cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmDelete(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleDelete}>
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Policy Preflight Dialog */}
      {preflightData && pendingActivation && (
        <PolicyPreflightDialog
          open={showPreflightDialog}
          onOpenChange={setShowPreflightDialog}
          title="Policy Validation - Activate Stack"
          description={`Review policy checks before activating stack "${pendingActivation.name}"`}
          checks={preflightData.checks}
          canProceed={preflightData.can_proceed}
          onProceed={doActivateStack}
          onCancel={() => {
            setShowPreflightDialog(false);
            setPendingActivation(null);
          }}
          isAdmin={false} // TODO: Get from user context
          isLoading={activateStack.isPending}
        />
      )}
    </>
  );
}
