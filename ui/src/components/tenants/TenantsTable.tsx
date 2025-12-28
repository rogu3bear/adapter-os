import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { VirtualizedTableRows } from '@/components/ui/virtualized-table';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { TenantsTableRow } from './TenantsTableRow';
import { Tenant as ApiTenant } from '@/api/types';
import { Upload, Download } from 'lucide-react';

export interface TenantsTableProps {
  tenants: ApiTenant[];
  selectedTenants: string[];
  onSelectedTenantsChange: (tenants: string[]) => void;
  onPause: (tenant: ApiTenant) => void;
  onEdit: (tenant: ApiTenant) => void;
  onAssignPolicies: (tenant: ApiTenant) => void;
  onAssignAdapters: (tenant: ApiTenant) => void;
  onViewUsage: (tenant: ApiTenant) => void;
  onArchive: (tenant: ApiTenant) => void;
  onImport: () => void;
  onExport: () => void;
  canManage: boolean;
}

export function TenantsTable({
  tenants,
  selectedTenants,
  onSelectedTenantsChange,
  onPause,
  onEdit,
  onAssignPolicies,
  onAssignAdapters,
  onViewUsage,
  onArchive,
  onImport,
  onExport,
  canManage,
}: TenantsTableProps) {
  const handleSelectAll = (checked: boolean | 'indeterminate') => {
    if (checked === true) {
      onSelectedTenantsChange(tenants.map((t) => t.id));
    } else {
      onSelectedTenantsChange([]);
    }
  };

  const handleSelectTenant = (tenantId: string, selected: boolean) => {
    if (selected) {
      onSelectedTenantsChange([...selectedTenants, tenantId]);
    } else {
      onSelectedTenantsChange(selectedTenants.filter((id) => id !== tenantId));
    }
  };

  const selectAllState =
    tenants.length === 0
      ? false
      : selectedTenants.length === tenants.length
        ? true
        : selectedTenants.length > 0
          ? 'indeterminate'
          : false;

  return (
    <Card className="p-4 rounded-lg border border-border bg-card shadow-md">
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle className="flex items-center gap-2">
            Active Workspaces
            <GlossaryTooltip termId="tenant" variant="icon" />
            <GlossaryTooltip termId="active-tenants">
              <span className="cursor-help text-muted-foreground text-sm">(Help)</span>
            </GlossaryTooltip>
          </CardTitle>
          <div className="flex gap-2">
            <GlossaryTooltip termId="import-tenants">
              <Button
                variant="outline"
                size="sm"
                onClick={onImport}
                disabled={!canManage}
              >
                <Upload className="h-4 w-4 mr-2" />
                Import
              </Button>
            </GlossaryTooltip>
            <GlossaryTooltip termId="export-tenants">
              <Button variant="outline" size="sm" onClick={onExport}>
                <Download className="h-4 w-4 mr-2" />
                Export
              </Button>
            </GlossaryTooltip>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        <div className="max-h-[600px] overflow-auto" data-virtual-container>
          <Table className="table-standard">
            <TableHeader>
              <TableRow>
                <TableHead className="table-cell-standard w-12">
                  <Checkbox
                    checked={selectAllState}
                    onCheckedChange={handleSelectAll}
                    aria-label="Select all workspaces"
                  />
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-name">
                    <span className="cursor-help">Name</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-id">
                    <span className="cursor-help">ID</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-isolation">
                    <span className="cursor-help">Isolation</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-users">
                    <span className="cursor-help">Users</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-adapters">
                    <span className="cursor-help">Adapters</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-status">
                    <span className="cursor-help">Status</span>
                  </GlossaryTooltip>
                </TableHead>
                <TableHead className="table-cell-standard">
                  <GlossaryTooltip termId="tenant-actions">
                    <span className="cursor-help">Actions</span>
                  </GlossaryTooltip>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <VirtualizedTableRows items={tenants} estimateSize={60}>
                {(tenant) => {
                  const tenantTyped = tenant as ApiTenant;
                  return (
                    <TenantsTableRow
                      key={tenantTyped.id}
                      tenant={tenantTyped}
                      isSelected={selectedTenants.includes(tenantTyped.id)}
                      onSelectionChange={(selected) =>
                        handleSelectTenant(tenantTyped.id, selected)
                      }
                      onPause={() => onPause(tenantTyped)}
                      onEdit={() => onEdit(tenantTyped)}
                      onAssignPolicies={() => onAssignPolicies(tenantTyped)}
                      onAssignAdapters={() => onAssignAdapters(tenantTyped)}
                      onViewUsage={() => onViewUsage(tenantTyped)}
                      onArchive={() => onArchive(tenantTyped)}
                      canManage={canManage}
                    />
                  );
                }}
              </VirtualizedTableRows>
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  );
}
