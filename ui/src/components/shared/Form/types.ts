import type { ReactNode } from "react";
import type {
  FieldPath,
  FieldValues,
  UseFormReturn,
  ControllerRenderProps,
  ControllerFieldState,
  UseFormStateReturn,
} from "react-hook-form";
import type { z } from "zod";

/**
 * Validation state for a form field
 */
export type ValidationState = "idle" | "validating" | "valid" | "invalid";

/**
 * Base props for form field components
 */
export interface FormFieldBaseProps {
  /** Unique field name/identifier */
  name: string;
  /** Label text displayed above the field */
  label?: string;
  /** Optional description/help text */
  description?: string;
  /** Placeholder text for the input */
  placeholder?: string;
  /** Whether the field is required */
  required?: boolean;
  /** Whether the field is disabled */
  disabled?: boolean;
  /** Additional CSS classes */
  className?: string;
}

/**
 * Props for text input fields
 */
export interface TextFieldProps extends FormFieldBaseProps {
  type?: "text" | "email" | "password" | "url" | "tel" | "search";
  autoComplete?: string;
  maxLength?: number;
  minLength?: number;
}

/**
 * Props for textarea fields
 */
export interface TextareaFieldProps extends FormFieldBaseProps {
  rows?: number;
  maxLength?: number;
  resize?: "none" | "vertical" | "horizontal" | "both";
}

/**
 * Props for number input fields
 */
export interface NumberFieldProps extends FormFieldBaseProps {
  min?: number;
  max?: number;
  step?: number;
}

/**
 * Props for select fields
 */
export interface SelectFieldProps extends FormFieldBaseProps {
  options: SelectOption[];
  multiple?: boolean;
}

/**
 * Option for select fields
 */
export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

/**
 * Props for checkbox fields
 */
export interface CheckboxFieldProps extends FormFieldBaseProps {
  /** Whether checkbox is in indeterminate state */
  indeterminate?: boolean;
}

/**
 * Field error information
 */
export interface FieldError {
  type: string;
  message?: string;
}

/**
 * Form field render props passed to custom render functions
 */
export interface FormFieldRenderProps<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> {
  field: ControllerRenderProps<TFieldValues, TName>;
  fieldState: ControllerFieldState;
  formState: UseFormStateReturn<TFieldValues>;
}

/**
 * Props for FormField component
 */
export interface FormFieldProps<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> extends FormFieldBaseProps {
  /** Form instance from useForm() */
  form: UseFormReturn<TFieldValues>;
  /** Field name (path in form values) */
  name: TName;
  /** Custom render function for the input */
  render?: (props: FormFieldRenderProps<TFieldValues, TName>) => ReactNode;
  /** Input type (if not using custom render) */
  type?: TextFieldProps["type"] | "number" | "textarea" | "select" | "checkbox";
  /** Options for select fields */
  options?: SelectOption[];
  /** Rows for textarea */
  rows?: number;
  /** Min value for number inputs */
  min?: number;
  /** Max value for number inputs */
  max?: number;
  /** Step for number inputs */
  step?: number;
  /** Auto-complete attribute */
  autoComplete?: string;
}

/**
 * Props for FormSection component
 */
export interface FormSectionProps {
  /** Section title */
  title?: string;
  /** Section description */
  description?: string;
  /** Child form fields */
  children: ReactNode;
  /** Additional CSS classes */
  className?: string;
  /** Whether to show a divider after the section */
  divider?: boolean;
  /** Whether section content is collapsible */
  collapsible?: boolean;
  /** Default collapsed state (only used if collapsible is true) */
  defaultCollapsed?: boolean;
}

/**
 * Props for FormActions component
 */
export interface FormActionsProps {
  /** Submit button text */
  submitText?: string;
  /** Cancel button text */
  cancelText?: string;
  /** Reset button text */
  resetText?: string;
  /** Whether form is submitting */
  isSubmitting?: boolean;
  /** Whether form is dirty (has changes) */
  isDirty?: boolean;
  /** Whether form is valid */
  isValid?: boolean;
  /** Callback when cancel is clicked */
  onCancel?: () => void;
  /** Callback when reset is clicked */
  onReset?: () => void;
  /** Whether to show cancel button */
  showCancel?: boolean;
  /** Whether to show reset button */
  showReset?: boolean;
  /** Additional CSS classes */
  className?: string;
  /** Button alignment */
  align?: "left" | "center" | "right" | "between";
  /** Button size variant */
  size?: "sm" | "default" | "lg";
}

/**
 * Zod schema type for form validation
 * Compatible with Zod v4 and @hookform/resolvers
 */
export type FormSchema<T> = z.ZodType<T>;

/**
 * Form configuration options
 */
export interface FormConfig<TSchema extends FieldValues> {
  /** Zod schema for validation */
  schema?: FormSchema<TSchema>;
  /** Default form values */
  defaultValues?: Partial<TSchema>;
  /** Validation mode */
  mode?: "onChange" | "onBlur" | "onSubmit" | "onTouched" | "all";
  /** Revalidation mode */
  reValidateMode?: "onChange" | "onBlur" | "onSubmit";
}

/**
 * Field config for dynamic form generation
 */
export interface FieldConfig extends FormFieldBaseProps {
  type: "text" | "email" | "password" | "number" | "textarea" | "select" | "checkbox";
  options?: SelectOption[];
  rows?: number;
  min?: number;
  max?: number;
  step?: number;
  autoComplete?: string;
  defaultValue?: unknown;
}

/**
 * Dynamic form fields configuration
 */
export type FormFieldsConfig = Record<string, FieldConfig>;
