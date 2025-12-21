/**
 * Schema validation utilities
 * Provides helper functions for form validation, error handling, and formatting
 */

import { ZodError, ZodSchema } from 'zod';

/**
 * Validation error with field-specific details
 */
export interface ValidationErrorDetail {
  field: string;
  message: string;
  suggestion?: string;
  code?: string;
}

/**
 * Validation result with all errors
 */
export interface ValidationResult {
  success: boolean;
  errors: ValidationErrorDetail[];
  fieldErrors: Record<string, string>;
}

/**
 * Format a Zod validation error into a user-friendly structure
 */
export function formatValidationError(error: ZodError): ValidationResult {
  const fieldErrors: Record<string, string> = {};
  const errors: ValidationErrorDetail[] = [];

  error.issues.forEach((issue) => {
    const field = issue.path.join('.');
    const message = issue.message;

    // Add to field errors map (for quick lookup)
    fieldErrors[field] = message;

    // Add detailed error info
    errors.push({
      field,
      message,
      code: issue.code,
      suggestion: getSuggestion(field, message, issue.code),
    });
  });

  return {
    success: false,
    errors,
    fieldErrors,
  };
}

/**
 * Get a helpful suggestion for a validation error
 */
function getSuggestion(field: string, message: string, code?: string): string | undefined {
  const suggestions: Record<string, Record<string, string>> = {
    name: {
      'too_small': 'Provide a more descriptive name (at least 3 characters)',
      'too_big': 'Shorten the name to 100 characters or less',
      'invalid': 'Use only alphanumeric characters, hyphens, and underscores',
    },
    prompt: {
      'too_small': 'Provide more context or detail in your prompt',
      'too_big': 'Consider breaking your prompt into smaller chunks',
      'invalid': 'Remove any invisible characters or control characters',
    },
    rank: {
      'too_small': 'Increase rank to at least 2 for better model adaptation',
      'too_big': 'Reduce rank to 256 or less to save memory',
    },
    maxSequenceLength: {
      'too_small': 'Increase sequence length to handle longer documents',
      'too_big': 'Reduce sequence length to save memory and speed up processing',
    },
  };

  return suggestions[field]?.[code || ''] || undefined;
}

/**
 * Parse validation errors from a ZodError
 * Returns an object suitable for react-hook-form setError
 */
export function parseValidationErrors(error: ZodError): Record<string, { message: string }> {
  const errors: Record<string, { message: string }> = {};

  error.issues.forEach((issue) => {
    const field = issue.path.join('.');
    if (!errors[field]) {
      errors[field] = { message: issue.message };
    }
  });

  return errors;
}

/**
 * Validate a single field against a schema
 * Useful for real-time field validation
 */
export function validateField<T extends Record<string, unknown>>(
  schema: ZodSchema,
  fieldName: string,
  value: unknown,
  context?: T
): { valid: boolean; error?: string; suggestion?: string } {
  try {
    // Extract the field schema from the parent schema
    const fieldSchema = (schema as { _shape?: Record<string, ZodSchema> })._shape?.[fieldName];
    if (!fieldSchema) {
      return { valid: true }; // Field not found in schema, skip validation
    }

    // Validate the field value
    fieldSchema.parse(value);
    return { valid: true };
  } catch (error) {
    if (error instanceof ZodError && error.issues.length > 0) {
      const issue = error.issues[0];
      return {
        valid: false,
        error: issue.message,
        suggestion: getSuggestion(fieldName, issue.message, issue.code),
      };
    }
    return { valid: true };
  }
}

/**
 * Format a validation error message for display
 * Combines error message and suggestion
 */
export function formatFieldError(fieldName: string, error: string, suggestion?: string): string {
  let message = error;

  // Add custom formatting for specific fields
  if (fieldName === 'prompt') {
    message = `Prompt: ${error}`;
  } else if (fieldName === 'name') {
    message = `Name: ${error}`;
  } else {
    message = `${fieldName}: ${error}`;
  }

  if (suggestion) {
    message += ` (Tip: ${suggestion})`;
  }

  return message;
}

/**
 * Batch validate multiple fields
 * Useful for validating entire forms
 */
export async function validateForm<T extends Record<string, unknown>>(
  schema: ZodSchema,
  data: T
): Promise<ValidationResult> {
  try {
    await schema.parseAsync(data);
    return {
      success: true,
      errors: [],
      fieldErrors: {},
    };
  } catch (error) {
    if (error instanceof ZodError) {
      return formatValidationError(error);
    }
    return {
      success: false,
      errors: [{ field: 'unknown', message: 'Validation failed' }],
      fieldErrors: { unknown: 'Validation failed' },
    };
  }
}

/**
 * Create error messages map for a form
 * Useful for displaying multiple errors at once
 */
export function getErrorMessages(result: ValidationResult): Record<string, string> {
  return result.fieldErrors;
}

/**
 * Check if a specific field has errors
 */
export function hasFieldError(result: ValidationResult, fieldName: string): boolean {
  return fieldName in result.fieldErrors;
}

/**
 * Get the error message for a specific field
 */
export function getFieldError(result: ValidationResult, fieldName: string): string | undefined {
  return result.fieldErrors[fieldName];
}
