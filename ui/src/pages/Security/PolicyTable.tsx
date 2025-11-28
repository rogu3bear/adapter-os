/**
 * PolicyTable - Table component for displaying and managing policies
 *
 * Features:
 * - Policy list with sorting and filtering
 * - Actions: View, Sign, Compare, Export
 * - Status badges (active, draft, archived)
 * - RBAC-aware action buttons
 */

import React from 'react';
import { DataTable } from '@/components/shared/DataTable/DataTable';
import type { ColumnDef } from '@/components/shared/DataTable/types';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { MoreHorizontal, Eye, FileSignature, GitCompare, Download } from 'lucide-react';
import type { Policy } from '@/api/types';

interface PolicyTableProps {
  policies: Policy[];
  onSelect: (policy: Policy) => void;
  onSign: (policy: Policy) => void;
  onCompare: (policy: Policy) => void;
  onExport: (policy: Policy) => void;
  canSign: boolean;
  isSigningPolicy: boolean;
}

export function PolicyTable({
  policies,
  onSelect,
  onSign,
  onCompare,
  onExport,
  canSign,
  isSigningPolicy,
}: PolicyTableProps) {
  const columns: ColumnDef<Policy>[] = [
    {
      id: 'cpid',
      header: 'Policy ID',
      accessorKey: 'cpid',
      cell: (context) => {
        const cpid = context.row.cpid || context.row.id;
        return (
          <div className="flex items-center gap-2">
            <span className="font-mono text-sm">{cpid}</span>
            {context.row.signature && (
              <Badge variant="outline" className="text-xs">
                Signed
              </Badge>
            )}
          </div>
        );
      },
      enableSorting: true,
    },
    {
      id: 'name',
      header: 'Name',
      accessorKey: 'name',
      enableSorting: true,
    },
    {
      id: 'type',
      header: 'Type',
      accessorKey: 'type',
      enableSorting: true,
    },
    {
      id: 'status',
      header: 'Status',
      accessorKey: 'status',
      cell: (context) => {
        const status = context.row.status;
        const enabled = context.row.enabled;

        const getStatusBadge = () => {
          if (status === 'active' || enabled) {
            return <Badge variant="default" className="bg-green-500">Active</Badge>;
          }
          if (status === 'draft') {
            return <Badge variant="secondary">Draft</Badge>;
          }
          if (status === 'archived') {
            return <Badge variant="outline">Archived</Badge>;
          }
          return <Badge variant="outline">{status || 'Unknown'}</Badge>;
        };

        return getStatusBadge();
      },
      enableSorting: true,
    },
    {
      id: 'created_at',
      header: 'Created',
      accessorKey: 'created_at',
      cell: (context) => {
        const date = context.row.created_at;
        return date ? new Date(date).toLocaleDateString() : '-';
      },
      enableSorting: true,
    },
    {
      id: 'updated_at',
      header: 'Updated',
      accessorKey: 'updated_at',
      cell: (context) => {
        const date = context.row.updated_at;
        return date ? new Date(date).toLocaleDateString() : '-';
      },
      enableSorting: true,
    },
    {
      id: 'actions',
      header: 'Actions',
      cell: (context) => {
        const policy = context.row;
        return (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm">
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={() => onSelect(policy)}>
                <Eye className="h-4 w-4 mr-2" />
                View Details
              </DropdownMenuItem>
              {canSign && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    onClick={() => onSign(policy)}
                    disabled={isSigningPolicy}
                  >
                    <FileSignature className="h-4 w-4 mr-2" />
                    Sign Policy
                  </DropdownMenuItem>
                </>
              )}
              <DropdownMenuItem onClick={() => onCompare(policy)}>
                <GitCompare className="h-4 w-4 mr-2" />
                Compare
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => onExport(policy)}>
                <Download className="h-4 w-4 mr-2" />
                Export
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        );
      },
    },
  ];

  return (
    <DataTable
      data={policies}
      columns={columns}
      getRowId={(row) => row.cpid || row.id}
      enableSorting
      enablePagination
      pagination={{ pageIndex: 0, pageSize: 10 }}
      pageSizes={[10, 25, 50, 100]}
      emptyTitle="No policies found"
      emptyDescription="No policies are currently registered in the system."
      onRowClick={(row) => onSelect(row)}
      className="border rounded-lg"
    />
  );
}
