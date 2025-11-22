# DataTable Component System

A comprehensive, type-safe data table component system for React with full sorting, filtering, pagination, and selection support.

## Status: Production Ready

**Location:** `/Users/star/Dev/aos/ui/src/components/shared/DataTable/`

**Total:** 11 files, 4,617 lines of code

## Quick Start

```tsx
import { DataTable, Column } from '@/components/shared/DataTable';

interface User {
  id: string;
  name: string;
  email: string;
}

const columns: Column<User>[] = [
  { id: 'name', header: 'Name', accessorKey: 'name', sortable: true },
  { id: 'email', header: 'Email', accessorKey: 'email' },
];

function MyTable() {
  const [users] = useState<User[]>([...]);

  return (
    <DataTable
      data={users}
      columns={columns}
      getRowId={(row) => row.id}
      enableSorting
      enablePagination
    />
  );
}
```

## Features

### Core Functionality
- Generic TypeScript types (`<TData, TValue>`)
- Sorting (ascending/descending/none)
- Filtering (column filters + global search)
- Pagination (client-side and server-side)
- Row selection (none/single/multi)
- Column visibility controls

### State Management
- **Controlled Mode:** Pass state and callbacks for full control
- **Uncontrolled Mode:** Component manages its own state
- **Hybrid Mode:** Control some state, let component handle the rest

### Data Processing
- **Client-side:** `useDataTable` hook processes data in-memory
- **Server-side:** `useDataTableServer` hook integrates with React Query

### UI/UX
- Loading skeleton states
- Empty state with customization
- Responsive design
- Striped/bordered/dense modes
- Sticky headers
- Row hover effects
- Selection count indicator

### Accessibility
- ARIA labels and roles
- Keyboard navigation
- Screen reader support
- Sort state announcements

## Components

### Main Component

**`<DataTable>`** - The primary table component

```tsx
<DataTable
  data={items}
  columns={columns}
  getRowId={(row) => row.id}
  // Selection
  selectionMode="multi"
  selectedIds={selectedIds}
  onSelectionChange={setSelectedIds}
  // Sorting
  enableSorting
  sorting={sortState}
  onSortingChange={setSortState}
  // Filtering
  filters={columnFilters}
  globalFilter={searchTerm}
  onFilterChange={setColumnFilters}
  // Pagination
  enablePagination
  pagination={paginationState}
  onPaginationChange={setPaginationState}
  // States
  isLoading={isLoading}
  // Styling
  striped
  bordered
  dense
  stickyHeader
  maxHeight="600px"
/>
```

### Sub-Components (Optional)

These can be used independently for custom layouts:

- **`<DataTableHeader>`** - Table header with sort controls
- **`<DataTableBody>`** - Table body with row rendering
- **`<DataTableRow>`** - Individual row component
- **`<DataTablePagination>`** - Pagination controls
- **`<DataTableToolbar>`** - Search, filters, bulk actions
- **`<DataTableFilters>`** - Advanced filtering UI

## Hooks

### `useDataTable` (Client-side)

Manages table state with in-memory data processing.

```tsx
const {
  processedData,        // Sorted, filtered, paginated data
  sortState,           // Current sort state
  toggleSort,          // Toggle column sort
  filterState,         // Current filters
  setGlobalFilter,     // Set global search
  paginationState,     // Current pagination
  setPageIndex,        // Change page
  selectionState,      // Selected row IDs
  toggleRowSelection,  // Toggle row selection
} = useDataTable({
  data: rawData,
  columns,
  getRowId: (row) => row.id,
  enablePagination: true,
  defaultPageSize: 25,
});
```

### `useDataTableServer` (Server-side)

Integrates with React Query for server-side operations.

```tsx
const {
  processedData,
  isLoading,
  isFetching,
  error,
  refetch,
  sortState,
  toggleSort,
  filterState,
  setGlobalFilter,
  paginationState,
  setPageIndex,
} = useDataTableServer({
  columns,
  getRowId: (row) => row.id,
  queryKey: ['users'],
  queryFn: async (params) => {
    const res = await fetchUsers(params);
    return {
      data: res.users,
      total: res.total,
      pageIndex: params.pagination.pageIndex,
      pageSize: params.pagination.pageSize,
    };
  },
});
```

## Column Definitions

```tsx
interface Column<TData, TValue = unknown> {
  id: string;                    // Unique column ID
  header: string | (() => ReactNode);  // Header content
  accessorKey?: keyof TData;     // Simple property accessor
  accessorFn?: (row: TData, index: number) => TValue;  // Custom accessor
  cell?: (info: CellContext<TData, TValue>) => ReactNode;  // Custom renderer
  sortable?: boolean;            // Enable sorting
  filterable?: boolean;          // Enable filtering
  sortingFn?: (a: TData, b: TData, direction: SortDirection) => number;
  filterMatcher?: (row: TData, filterValue: string, cellValue: TValue) => boolean;
  width?: string;                // CSS width
  minWidth?: string;             // CSS min-width
  maxWidth?: string;             // CSS max-width
  align?: 'left' | 'center' | 'right';
  visible?: boolean;             // Show/hide column
  headerClassName?: string;      // Header CSS classes
  cellClassName?: string;        // Cell CSS classes
  sticky?: 'left' | 'right';     // Sticky positioning
}
```

## Examples

### Basic Table

```tsx
import { DataTable, Column } from '@/components/shared/DataTable';

interface Product {
  id: string;
  name: string;
  price: number;
  stock: number;
}

const columns: Column<Product>[] = [
  {
    id: 'name',
    header: 'Product Name',
    accessorKey: 'name',
    sortable: true,
  },
  {
    id: 'price',
    header: 'Price',
    accessorKey: 'price',
    cell: ({ value }) => `$${value.toFixed(2)}`,
    sortable: true,
  },
  {
    id: 'stock',
    header: 'Stock',
    accessorKey: 'stock',
    cell: ({ value }) => (
      <span className={value < 10 ? 'text-red-500' : ''}>
        {value}
      </span>
    ),
  },
];

function ProductTable() {
  const [products] = useState<Product[]>([...]);

  return (
    <DataTable
      data={products}
      columns={columns}
      getRowId={(row) => row.id}
      enableSorting
      striped
    />
  );
}
```

### With Custom Hook (Client-side)

```tsx
function AdvancedTable() {
  const [data] = useState<Product[]>([...]);

  const {
    processedData,
    sortState,
    toggleSort,
    filterState,
    setGlobalFilter,
    paginationState,
    paginationMeta,
    setPageIndex,
    setPageSize,
    selectionState,
    toggleRowSelection,
  } = useDataTable({
    data,
    columns,
    getRowId: (row) => row.id,
    enablePagination: true,
    defaultPageSize: 10,
  });

  return (
    <div className="space-y-4">
      <DataTableToolbar
        searchValue={filterState.globalFilter?.value}
        onSearch={setGlobalFilter}
        selectedCount={selectionState.selectedIds.size}
      />

      <DataTable
        data={processedData}
        columns={columns}
        getRowId={(row) => row.id}
        sorting={sortState}
        onSortingChange={toggleSort}
        selectionMode="multi"
        selectedIds={selectionState.selectedIds}
        onSelectionChange={toggleRowSelection}
      />

      <DataTablePagination
        pagination={paginationState}
        meta={paginationMeta}
        onPageChange={setPageIndex}
        onPageSizeChange={setPageSize}
      />
    </div>
  );
}
```

### Server-Side with React Query

```tsx
import { useDataTableServer } from '@/components/shared/DataTable';
import { apiClient } from '@/api/client';

function ServerTable() {
  const {
    processedData,
    isLoading,
    error,
    sortState,
    toggleSort,
    filterState,
    setGlobalFilter,
    paginationState,
    setPageIndex,
    setPageSize,
    refetch,
  } = useDataTableServer({
    columns,
    getRowId: (row) => row.id,
    queryKey: ['products'],
    queryFn: async ({ sort, filter, pagination }) => {
      const response = await apiClient.get('/products', {
        params: {
          sortBy: sort?.columnId,
          sortDir: sort?.direction,
          search: filter.globalFilter?.value,
          page: pagination.pageIndex,
          pageSize: pagination.pageSize,
        },
      });

      return {
        data: response.data.items,
        total: response.data.total,
        pageIndex: pagination.pageIndex,
        pageSize: pagination.pageSize,
      };
    },
  });

  if (error) {
    return <div>Error loading data</div>;
  }

  return (
    <div>
      <input
        type="text"
        placeholder="Search..."
        onChange={(e) => setGlobalFilter(e.target.value)}
      />

      <DataTable
        data={processedData}
        columns={columns}
        getRowId={(row) => row.id}
        sorting={sortState}
        onSortingChange={toggleSort}
        isLoading={isLoading}
      />

      <DataTablePagination
        pagination={paginationState}
        onPageChange={setPageIndex}
        onPageSizeChange={setPageSize}
      />
    </div>
  );
}
```

## Advanced Features

### Custom Cell Renderers

```tsx
const columns: Column<User>[] = [
  {
    id: 'status',
    header: 'Status',
    accessorKey: 'status',
    cell: ({ value }) => (
      <Badge variant={value === 'active' ? 'success' : 'default'}>
        {value}
      </Badge>
    ),
  },
  {
    id: 'actions',
    header: 'Actions',
    cell: ({ row }) => (
      <DropdownMenu>
        <DropdownMenuTrigger>Actions</DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem onClick={() => edit(row.id)}>
            Edit
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => delete(row.id)}>
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    ),
  },
];
```

### Custom Sorting

```tsx
{
  id: 'date',
  header: 'Date',
  accessorKey: 'createdAt',
  sortingFn: (a, b, direction) => {
    const dateA = new Date(a.createdAt).getTime();
    const dateB = new Date(b.createdAt).getTime();
    const comparison = dateA - dateB;
    return direction === 'asc' ? comparison : -comparison;
  },
}
```

### Custom Filtering

```tsx
{
  id: 'tags',
  header: 'Tags',
  accessorKey: 'tags',
  filterMatcher: (row, filterValue, cellValue) => {
    const tags = cellValue as string[];
    return tags.some(tag =>
      tag.toLowerCase().includes(filterValue.toLowerCase())
    );
  },
}
```

## Performance Tips

1. **Memoize columns** - Use `useMemo` for column definitions
2. **Stable getRowId** - Use consistent row ID extraction
3. **Pagination** - Enable pagination for large datasets (1000+ rows)
4. **Server-side** - Use `useDataTableServer` for very large datasets (10k+ rows)
5. **Virtual scrolling** - Consider adding for 10k+ rows in viewport

## Type Safety

All components are fully type-safe with generics:

```tsx
// Type inference works automatically
const columns: Column<User>[] = [...];
const { processedData } = useDataTable<User>({ data, columns, ... });
// processedData is correctly typed as User[]
```

## Migration from TanStack Table

If migrating from TanStack Table v8:

| TanStack Table | DataTable |
|----------------|-----------|
| `useReactTable` | `useDataTable` |
| `ColumnDef` | `Column` |
| `getCoreRowModel()` | Built-in |
| `getSortedRowModel()` | Built-in |
| `getFilteredRowModel()` | Built-in |
| `getPaginationRowModel()` | Built-in |

## File Structure

```
DataTable/
├── DataTable.tsx              # Main component (727 lines)
├── types.ts                   # Type definitions (535 lines)
├── useDataTable.ts            # State management hooks (1,303 lines)
├── index.ts                   # Barrel exports (158 lines)
├── DataTableHeader.tsx        # Header component (200+ lines)
├── DataTableBody.tsx          # Body component (180+ lines)
├── DataTableRow.tsx           # Row component (150+ lines)
├── DataTablePagination.tsx    # Pagination component (150+ lines)
├── DataTableToolbar.tsx       # Toolbar component (250+ lines)
├── DataTableFilters.tsx       # Filters component (450+ lines)
├── hooks.ts                   # Legacy utility hooks (250+ lines)
└── README.md                  # This file
```

## Dependencies

- React 18+
- shadcn/ui components (Table, Button, Checkbox, Select, Skeleton)
- Lucide React (icons)
- @tanstack/react-query (for server-side hook)

## Browser Support

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## License

© 2025 JKCA / James KC Auchterlonie. All rights reserved.

---

For issues or questions, refer to the AdapterOS documentation or contact the development team.
