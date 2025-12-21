/**
 * Authentication Form Types
 *
 * Form state and validation types for authentication flows.
 * Maps to schemas in ui/src/schemas/common.schema.ts
 *
 * Citations:
 * - ui/src/components/login/LoginCredentialsForm.tsx - Login form implementation
 * - ui/src/schemas/common.schema.ts - LoginFormSchema
 * - ui/src/hooks/auth/useAuthFlow.ts - Auth flow logic
 */

/**
 * Login form data
 *
 * Derived from LoginFormSchema (ui/src/schemas/common.schema.ts)
 */
export interface LoginFormData {
  email: string;
  password: string;
  totp?: string;
}

/**
 * Login credentials (processed form data)
 */
export interface LoginCredentials {
  email: string;
  password: string;
  totp?: string;
}

/**
 * Registration form data
 */
export interface RegistrationFormData {
  email: string;
  password: string;
  confirmPassword: string;
  displayName?: string;
  role?: string;
  tenantId?: string;
}

/**
 * Password reset request form data
 */
export interface PasswordResetRequestFormData {
  email: string;
}

/**
 * Password reset confirmation form data
 */
export interface PasswordResetConfirmFormData {
  token: string;
  newPassword: string;
  confirmPassword: string;
}

/**
 * TOTP setup form data
 */
export interface TotpSetupFormData {
  totpCode: string;
  secret: string;
}

/**
 * Auth form validation state
 */
export interface AuthFormValidationState {
  isValid: boolean;
  errors: Record<string, string>;
  touched: Record<string, boolean>;
  isSubmitting: boolean;
}

/**
 * Complete login form state
 */
export interface LoginFormState {
  formData: LoginFormData;
  validation: AuthFormValidationState;
  showTotpField: boolean;
  lockoutMessage?: string;
  remainingAttempts?: number;
}

/**
 * Complete registration form state
 */
export interface RegistrationFormState {
  formData: RegistrationFormData;
  validation: AuthFormValidationState;
  step?: number; // For multi-step registration
}

/**
 * User profile edit form data
 */
export interface UserProfileFormData {
  displayName?: string;
  email?: string;
  currentPassword?: string;
  newPassword?: string;
  confirmPassword?: string;
}

/**
 * User profile form state
 */
export interface UserProfileFormState {
  formData: UserProfileFormData;
  validation: AuthFormValidationState;
  hasChanges: boolean;
}
