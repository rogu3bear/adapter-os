/**
 * Workspace ID used in UI, sourced from API tenant_id fields.
 */
export type WorkspaceId = string;

export function workspaceIdFromTenantId(tenantId?: string | null): WorkspaceId {
  return tenantId ?? '';
}
