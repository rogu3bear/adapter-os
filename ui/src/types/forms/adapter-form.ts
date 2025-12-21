/**
 * Adapter Form Types
 *
 * Form state and validation types for adapter creation and editing.
 *
 * Citations:
 * - ui/src/pages/Adapters/AdapterRegisterPage.tsx - Adapter registration form
 * - ui/src/components/AdapterImportWizard.tsx - Import workflow
 */

/**
 * Adapter creation form data
 */
export interface AdapterCreateFormData {
  // Basic info
  name: string;
  description?: string;
  category: 'code' | 'framework' | 'codebase' | 'ephemeral';

  // Semantic naming (optional, for structured naming)
  tenant?: string;
  domain?: string;
  purpose?: string;
  revision?: string;

  // Source
  sourceType: 'upload' | 'import' | 'train';
  filePath?: string;
  importUrl?: string;

  // Metadata
  tags?: string[];
  version?: string;
}

/**
 * Adapter edit form data
 */
export interface AdapterEditFormData {
  adapterId: string;
  name?: string;
  description?: string;
  tags?: string[];
  pinned?: boolean;
}

/**
 * Adapter registration form data
 */
export interface AdapterRegisterFormData {
  adapterId: string;
  adapterPath: string;
  manifestPath?: string;
  autoLoad?: boolean;
}

/**
 * Adapter import form data
 */
export interface AdapterImportFormData {
  importUrl: string;
  targetName?: string;
  validateSignature: boolean;
  autoRegister: boolean;
}

/**
 * Adapter form validation state
 */
export interface AdapterFormValidationState {
  isValid: boolean;
  errors: Record<string, string>;
  touched: Record<string, boolean>;
  isSubmitting: boolean;
  warnings?: Record<string, string>;
}

/**
 * Complete adapter create form state
 */
export interface AdapterCreateFormState {
  formData: AdapterCreateFormData;
  validation: AdapterFormValidationState;
  uploadProgress?: number;
  currentStep?: number;
}

/**
 * Complete adapter edit form state
 */
export interface AdapterEditFormState {
  formData: AdapterEditFormData;
  validation: AdapterFormValidationState;
  originalData?: AdapterEditFormData;
  hasChanges: boolean;
}

/**
 * Stack form data (adapter stack configuration)
 */
export interface StackFormData {
  name: string;
  description?: string;
  adapters: Array<{
    adapter_id: string;
    gate: number; // Q15 quantized (0-32767)
  }>;
}

/**
 * Stack form state
 */
export interface StackFormState {
  formData: StackFormData;
  validation: AdapterFormValidationState;
  totalGate?: number;
}
