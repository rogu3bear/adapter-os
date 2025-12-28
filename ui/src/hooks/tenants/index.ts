// Tenants hooks barrel file

// Data fetching hook
export { useTenantsData } from './useTenantsData';
export type { UseTenantsDataOptions, UseTenantsDataReturn } from './useTenantsData';

// CRUD operations hook
export { useTenantOperations } from './useTenantOperations';
export type {
  TenantOperationCallbacks,
  UseTenantOperationsOptions,
  UseTenantOperationsReturn,
} from './useTenantOperations';

// Bulk actions hook
export { useTenantBulkActions } from './useTenantBulkActions';
export type {
  BulkActionCallbacks,
  UseTenantBulkActionsOptions,
  UseTenantBulkActionsReturn,
} from './useTenantBulkActions';
