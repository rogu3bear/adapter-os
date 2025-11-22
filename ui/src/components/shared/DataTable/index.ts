/**
 * DataTable - A flexible, reusable data table component
 *
 * Features:
 * - Generic typing for row data
 * - Column definitions with sorting and filtering
 * - Selection support (single/multi)
 * - Responsive design with Tailwind
 * - Integration points for pagination
 * - Empty state handling
 * - Loading state
 *
 * @example
 * ```tsx
 * import { DataTable, ColumnDef } from '@/components/shared/DataTable';
 *
 * interface User {
 *   id: string;
 *   name: string;
 *   email: string;
 *   role: string;
 * }
 *
 * const columns: ColumnDef<User>[] = [
 *   {
 *     id: 'name',
 *     header: 'Name',
 *     accessorKey: 'name',
 *     enableSorting: true,
 *   },
 *   {
 *     id: 'email',
 *     header: 'Email',
 *     accessorKey: 'email',
 *   },
 *   {
 *     id: 'role',
 *     header: 'Role',
 *     accessorKey: 'role',
 *     cell: ({ value }) => <Badge>{value}</Badge>,
 *   },
 * ];
 *
 * function UserTable() {
 *   const [users, setUsers] = useState<User[]>([]);
 *   const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
 *
 *   return (
 *     <DataTable
 *       data={users}
 *       columns={columns}
 *       getRowId={(row) => row.id}
 *       selectionMode="multi"
 *       selectedIds={selectedIds}
 *       onSelectionChange={setSelectedIds}
 *       enableSorting
 *       striped
 *       bordered
 *     />
 *   );
 * }
 * ```
 */

// Main DataTable component
export { DataTable, default } from "./DataTable";

// Types from types.ts
export type {
  // Sort types
  SortDirection,
  SortState,
  SortingState,
  MultiSortState,
  // Filter types
  FilterOperator,
  ColumnFilter,
  GlobalFilter,
  FilterState,
  // Pagination types
  PaginationState,
  PaginationMeta,
  // Selection types
  SelectionMode,
  SelectionState,
  RowSelection,
  SelectionCallbacks,
  // Column types
  CellContext,
  Column,
  ColumnDef,
  // DataTable props
  ServerSideConfig,
  DataTableProps,
  DataTableRef,
  // Hook return types
  UseDataTableReturn,
  // Server-side types
  ServerSideParams,
  ServerSideResponse,
  QueryKeyFactory,
  QueryFn,
} from "./types";

// Main useDataTable hook with React Query integration
export {
  useDataTable,
  useDataTableServer,
  type UseDataTableOptions,
  type UseDataTableServerOptions,
} from "./useDataTable";

// Legacy hooks for external use (backwards compatibility)
export {
  usePagination,
  useProcessedData,
  useRowSelection,
  useSorting,
} from "./hooks";

// Supporting components
export {
  DataTableHeader,
  type DataTableHeaderProps,
} from "./DataTableHeader";

export {
  DataTableRow,
  type DataTableRowProps,
} from "./DataTableRow";

export {
  DataTableBody,
  type DataTableBodyProps,
} from "./DataTableBody";

export {
  DataTablePagination,
  type DataTablePaginationProps,
} from "./DataTablePagination";

export {
  DataTableToolbar,
  type DataTableToolbarProps,
  type FilterOption,
  type BulkAction,
  type ExportFormat,
} from "./DataTableToolbar";

export {
  DataTableFilters,
  FilterChip,
  type DataTableFiltersProps,
  type FilterDefinition,
  type ActiveFilter,
  type SavedFilterSet,
} from "./DataTableFilters";
