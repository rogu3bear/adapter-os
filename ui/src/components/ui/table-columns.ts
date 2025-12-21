/**
 * Table Column Standardization Guide
 *
 * Standard column orders for each entity type to ensure consistent UI.
 * Rules:
 * - ID/Name column is always first
 * - Status comes before metrics
 * - Actions column is always last
 */

// Adapter table columns
export const ADAPTER_COLUMNS = ['name', 'tier', 'rank', 'lifecycle', 'state', 'memory', 'activation', 'actions'] as const;

// Training job table columns
export const TRAINING_JOB_COLUMNS = ['id', 'dataset', 'status', 'progress', 'loss', 'created', 'actions'] as const;

// Tenant table columns
export const TENANT_COLUMNS = ['name', 'id', 'isolation', 'users', 'adapters', 'status', 'actions'] as const;

// Audit log table columns
export const AUDIT_COLUMNS = ['timestamp', 'level', 'event', 'user', 'details'] as const;

// Node table columns
export const NODE_COLUMNS = ['hostname', 'status', 'cpu', 'memory', 'gpu', 'lastSeen', 'actions'] as const;

// Type exports for type-safe column references
export type AdapterColumn = typeof ADAPTER_COLUMNS[number];
export type TrainingJobColumn = typeof TRAINING_JOB_COLUMNS[number];
export type TenantColumn = typeof TENANT_COLUMNS[number];
export type AuditColumn = typeof AUDIT_COLUMNS[number];
export type NodeColumn = typeof NODE_COLUMNS[number];

// Header name mappings for consistent labeling
export const COLUMN_HEADERS: Record<string, string> = {
  // Common
  name: 'Name',
  id: 'ID',
  status: 'Status',
  actions: 'Actions',

  // Adapter specific
  tier: 'Tier',
  rank: 'Rank',
  lifecycle: 'Lifecycle',
  state: 'State',
  memory: 'Memory',
  activation: 'Activation',

  // Training specific
  dataset: 'Dataset',
  progress: 'Progress',
  loss: 'Loss',
  created: 'Created',

  // Tenant specific
  isolation: 'Isolation',
  users: 'Users',
  adapters: 'Adapters',

  // Audit specific
  timestamp: 'Timestamp',
  level: 'Level',
  event: 'Event',
  user: 'User',
  details: 'Details',

  // Node specific
  hostname: 'Hostname',
  cpu: 'CPU',
  gpu: 'GPU',
  lastSeen: 'Last Seen',
};
