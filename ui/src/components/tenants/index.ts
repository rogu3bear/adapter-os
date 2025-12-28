// Tenants components barrel file

// Main component
export { Tenants } from './Tenants';
export type { TenantsProps } from './Tenants';

// Table components
export { TenantsTable } from './TenantsTable';
export type { TenantsTableProps } from './TenantsTable';

export { TenantsTableRow } from './TenantsTableRow';
export type { TenantsTableRowProps } from './TenantsTableRow';

export { TenantsStatusBadge, ClassificationBadge } from './TenantsStatusBadge';
export type {
  TenantsStatusBadgeProps,
  ClassificationBadgeProps,
} from './TenantsStatusBadge';

// Header & Stats
export { TenantsHeader } from './TenantsHeader';
export type { TenantsHeaderProps } from './TenantsHeader';

export { TenantsKpiCards } from './TenantsKpiCards';
export type { TenantsKpiCardsProps } from './TenantsKpiCards';

// Dialogs
export { CreateTenantDialog } from './CreateTenantDialog';
export type { CreateTenantDialogProps, NewTenantData } from './CreateTenantDialog';

export { EditTenantDialog } from './EditTenantDialog';
export type { EditTenantDialogProps } from './EditTenantDialog';

export { TenantUsageDialog } from './TenantUsageDialog';
export type { TenantUsageDialogProps } from './TenantUsageDialog';

export { AssignPoliciesDialog } from './AssignPoliciesDialog';
export type { AssignPoliciesDialogProps } from './AssignPoliciesDialog';

export { AssignAdaptersDialog } from './AssignAdaptersDialog';
export type { AssignAdaptersDialogProps } from './AssignAdaptersDialog';

export { ArchiveTenantDialog } from './ArchiveTenantDialog';
export type { ArchiveTenantDialogProps } from './ArchiveTenantDialog';
