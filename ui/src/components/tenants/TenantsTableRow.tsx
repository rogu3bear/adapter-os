import React from 'react';
import { TableCell, TableRow } from '@/components/ui/table';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { BookmarkButton } from '@/components/ui/bookmark-button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { TenantsStatusBadge } from './TenantsStatusBadge';
import { Tenant as ApiTenant } from '@/api/types';
import {
  MoreHorizontal,
  Edit,
  Lock,
  Shield,
  Layers,
  BarChart3,
  Archive,
  UserCheck,
  Network,
} from 'lucide-react';

export interface TenantsTableRowProps {
  tenant: ApiTenant;
  isSelected: boolean;
  onSelectionChange: (selected: boolean) => void;
  onPause: () => void;
  onEdit: () => void;
  onAssignPolicies: () => void;
  onAssignAdapters: () => void;
  onViewUsage: () => void;
  onArchive: () => void;
  canManage: boolean;
}

export function TenantsTableRow({
  tenant,
  isSelected,
  onSelectionChange,
  onPause,
  onEdit,
  onAssignPolicies,
  onAssignAdapters,
  onViewUsage,
  onArchive,
  canManage,
}: TenantsTableRowProps) {
  const canPause =
    tenant.status !== 'paused' && tenant.status !== 'archived';

  return (
    <TableRow>
      <TableCell className="p-4 border-b border-border">
        <Checkbox
          checked={isSelected}
          onCheckedChange={(checked) => onSelectionChange(!!checked)}
          aria-label={`Select ${tenant.name}`}
        />
      </TableCell>
      <TableCell className="p-4 border-b border-border" role="gridcell">
        <div>
          <p className="font-medium">{tenant.name}</p>
          <p className="text-sm text-muted-foreground">
            {tenant.description || 'No description'}
          </p>
        </div>
      </TableCell>
      <TableCell
        className="p-4 border-b border-border text-sm text-muted-foreground"
        role="gridcell"
      >
        {tenant.id}
      </TableCell>
      <TableCell className="p-4 border-b border-border" role="gridcell">
        <div className="status-indicator status-neutral">
          {tenant.isolation_level || 'standard'}
        </div>
      </TableCell>
      <TableCell className="p-4 border-b border-border" role="gridcell">
        <div className="flex items-center gap-1">
          <UserCheck className="h-3 w-3 text-muted-foreground" />
          <span>{tenant.users || 0}</span>
        </div>
      </TableCell>
      <TableCell className="p-4 border-b border-border" role="gridcell">
        <div className="flex items-center gap-1">
          <Network className="h-3 w-3 text-muted-foreground" />
          <span>{tenant.adapters || 0}</span>
        </div>
      </TableCell>
      <TableCell className="p-4 border-b border-border" role="gridcell">
        <TenantsStatusBadge status={tenant.status} />
      </TableCell>
      <TableCell className="p-4 border-b border-border" role="gridcell">
        <div className="flex items-center gap-1">
          <BookmarkButton
            type="tenant"
            title={tenant.name}
            url={`/tenants?tenant=${encodeURIComponent(tenant.id)}`}
            entityId={tenant.id}
            description={
              tenant.description ||
              `Workspace - ${tenant.isolation_level || 'Unknown isolation'}`
            }
            variant="ghost"
            size="icon"
          />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                aria-label={`Actions for ${tenant.name}`}
              >
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {canPause && (
                <DropdownMenuItem onClick={onPause} disabled={!canManage}>
                  <Lock className="mr-2 h-4 w-4" />
                  Pause
                </DropdownMenuItem>
              )}
              <DropdownMenuItem onClick={onEdit} disabled={!canManage}>
                <Edit className="mr-2 h-4 w-4" />
                Edit
              </DropdownMenuItem>
              <DropdownMenuItem onClick={onAssignPolicies} disabled={!canManage}>
                <Shield className="mr-2 h-4 w-4" />
                Assign Policies
              </DropdownMenuItem>
              <DropdownMenuItem onClick={onAssignAdapters} disabled={!canManage}>
                <Layers className="mr-2 h-4 w-4" />
                Assign Adapters
              </DropdownMenuItem>
              <DropdownMenuItem onClick={onViewUsage}>
                <BarChart3 className="mr-2 h-4 w-4" />
                View Usage
              </DropdownMenuItem>
              <DropdownMenuItem onClick={onArchive} disabled={!canManage}>
                <Archive className="mr-2 h-4 w-4 text-red-600" />
                Archive
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </TableCell>
    </TableRow>
  );
}
