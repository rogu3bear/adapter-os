/**
 * Admin Form Types
 *
 * Form state and validation types for admin operations.
 * Maps to schemas in ui/src/schemas/admin.schema.ts
 *
 * Citations:
 * - ui/src/schemas/admin.schema.ts - Admin form schemas
 * - ui/src/pages/Admin/UserFormModal.tsx - User form implementation
 * - ui/src/pages/Admin/TenantFormModal.tsx - Tenant form implementation
 */

/**
 * User form data
 *
 * Derived from UserFormSchema (ui/src/schemas/admin.schema.ts)
 */
export interface UserFormData {
  email: string;
  password?: string;
  display_name?: string;
  role: 'developer' | 'admin' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';
  tenant_id?: string;
}

/**
 * Tenant form data
 *
 * Derived from TenantFormSchema (ui/src/schemas/admin.schema.ts)
 */
export interface TenantFormData {
  name: string;
  description?: string;
  uid?: number;
  gid?: number;
  isolation_level?: 'standard' | 'enhanced' | 'strict';
}

/**
 * Admin form validation state
 */
export interface AdminFormValidationState {
  isValid: boolean;
  errors: Record<string, string>;
  touched: Record<string, boolean>;
  isSubmitting: boolean;
}

/**
 * User form state
 */
export interface UserFormState {
  formData: UserFormData;
  validation: AdminFormValidationState;
  isEdit: boolean;
  userId?: string;
}

/**
 * Tenant form state
 */
export interface TenantFormState {
  formData: TenantFormData;
  validation: AdminFormValidationState;
  isEdit: boolean;
  tenantId?: string;
}

/**
 * Policy configuration form data
 */
export interface PolicyConfigFormData {
  policyId: string;
  enabled: boolean;
  config: Record<string, unknown>;
  customRules?: Array<{
    id: string;
    condition: string;
    action: string;
  }>;
}

/**
 * Policy form state
 */
export interface PolicyFormState {
  formData: PolicyConfigFormData;
  validation: AdminFormValidationState;
  isDirty: boolean;
}

/**
 * Workspace form data
 */
export interface WorkspaceFormData {
  name: string;
  description?: string;
  tenantId: string;
  settings?: Record<string, unknown>;
}

/**
 * Workspace form state
 */
export interface WorkspaceFormState {
  formData: WorkspaceFormData;
  validation: AdminFormValidationState;
  isEdit: boolean;
}
