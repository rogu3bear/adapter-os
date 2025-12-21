/**
 * Common Form Types
 * UI-specific types for form components and validation
 */

export interface FormFieldProps {
  name: string;
  label?: string;
  description?: string;
  error?: string;
  required?: boolean;
  disabled?: boolean;
  className?: string;
}

export interface InputProps extends FormFieldProps {
  type?: 'text' | 'email' | 'password' | 'number' | 'url' | 'tel' | 'search';
  placeholder?: string;
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  onBlur?: () => void;
  autoComplete?: string;
  maxLength?: number;
  minLength?: number;
}

export interface TextAreaProps extends FormFieldProps {
  placeholder?: string;
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  onBlur?: () => void;
  rows?: number;
  maxLength?: number;
  minLength?: number;
  resize?: boolean;
}

export interface SelectOption {
  label: string;
  value: string;
  disabled?: boolean;
  description?: string;
}

export interface SelectProps extends FormFieldProps {
  options: SelectOption[];
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  placeholder?: string;
  searchable?: boolean;
  clearable?: boolean;
  multiple?: boolean;
}

export interface CheckboxProps extends FormFieldProps {
  checked?: boolean;
  defaultChecked?: boolean;
  onChange?: (checked: boolean) => void;
}

export interface RadioGroupOption {
  label: string;
  value: string;
  description?: string;
  disabled?: boolean;
}

export interface RadioGroupProps extends FormFieldProps {
  options: RadioGroupOption[];
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  orientation?: 'horizontal' | 'vertical';
}

export interface SwitchProps extends FormFieldProps {
  checked?: boolean;
  defaultChecked?: boolean;
  onChange?: (checked: boolean) => void;
}

export interface FileInputProps extends FormFieldProps {
  accept?: string;
  multiple?: boolean;
  maxSize?: number;
  onFileSelect?: (files: File[]) => void;
  currentFiles?: File[];
}

export interface FormValidationError {
  field: string;
  message: string;
  code?: string;
}

export interface FormState<T = any> {
  values: T;
  errors: Record<string, string>;
  touched: Record<string, boolean>;
  isSubmitting: boolean;
  isValid: boolean;
  isDirty: boolean;
}

export interface FormSubmitHandler<T = any> {
  (values: T): void | Promise<void>;
}

export interface FormProps<T = any> {
  initialValues?: Partial<T>;
  onSubmit: FormSubmitHandler<T>;
  validate?: (values: T) => Record<string, string>;
  children: React.ReactNode;
  className?: string;
}

export interface DatePickerProps extends FormFieldProps {
  value?: Date;
  defaultValue?: Date;
  onChange?: (date: Date | undefined) => void;
  minDate?: Date;
  maxDate?: Date;
  disabledDates?: Date[];
  format?: string;
}

export interface SliderProps extends FormFieldProps {
  value?: number;
  defaultValue?: number;
  onChange?: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  showValue?: boolean;
}
