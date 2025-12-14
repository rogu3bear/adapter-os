/**
 * useZodFormValidation hook
 * Advanced validation hook that integrates with react-hook-form
 */

import { useCallback } from 'react';
import { ZodSchema, ZodError } from 'zod';
import { UseFormSetError, FieldValues, Path } from 'react-hook-form';
import { formatValidationError, ValidationResult } from '@/schemas/utils';

export interface UseZodValidationOptions<T extends FieldValues> {
  // react-hook-form setError function for integration
  setError?: UseFormSetError<T>;
  // Enable async validation
  async?: boolean;
}

export interface UseZodValidationResult<T extends FieldValues> {
  // Validate entire form and set errors in react-hook-form
  validateWithForm: (data: T) => Promise<ValidationResult>;

  // Validate and return result without setting form errors
  validateOnly: (data: T) => Promise<ValidationResult>;

  // Validate single field
  validateSingleField: (fieldName: Path<T>, value: unknown) => Promise<boolean>;
}

/**
 * Hook for Zod validation with react-hook-form integration
 */
export function useZodFormValidation<T extends FieldValues>(
  schema: ZodSchema,
  options: UseZodValidationOptions<T> = {}
): UseZodValidationResult<T> {
  const { setError, async: useAsync = true } = options;

  /**
   * Validate form and set errors in react-hook-form
   */
  const validateWithForm = useCallback(
    async (data: T): Promise<ValidationResult> => {
      try {
        if (useAsync) {
          await schema.parseAsync(data);
        } else {
          schema.parse(data);
        }
        return {
          success: true,
          errors: [],
          fieldErrors: {},
        };
      } catch (error) {
        if (error instanceof ZodError && setError) {
          const result = formatValidationError(error);

          // Set errors in react-hook-form
          result.errors.forEach((err) => {
            setError(err.field as Path<T>, {
              type: 'manual',
              message: err.message,
            });
          });

          return result;
        }
        return {
          success: false,
          errors: [{ field: 'unknown', message: 'Validation failed' }],
          fieldErrors: { unknown: 'Validation failed' },
        };
      }
    },
    [schema, setError, useAsync]
  );

  /**
   * Validate and return result without setting form errors
   */
  const validateOnly = useCallback(
    async (data: T): Promise<ValidationResult> => {
      try {
        if (useAsync) {
          await schema.parseAsync(data);
        } else {
          schema.parse(data);
        }
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
    },
    [schema, useAsync]
  );

  /**
   * Validate single field
   */
  const validateSingleField = useCallback(
    async (fieldName: Path<T>, value: unknown): Promise<boolean> => {
      try {
        const schemaObj = schema as { _shape?: Record<string, ZodSchema> };
        const fieldSchema = schemaObj._shape?.[fieldName];
        if (!fieldSchema) {
          return true; // Field not in schema
        }

        if (useAsync) {
          await fieldSchema.parseAsync(value);
        } else {
          fieldSchema.parse(value);
        }
        return true;
      } catch (error) {
        if (error instanceof ZodError && setError) {
          const issue = error.issues[0];
          setError(fieldName, {
            type: 'manual',
            message: issue.message,
          });
        }
        return false;
      }
    },
    [schema, setError, useAsync]
  );

  return {
    validateWithForm,
    validateOnly,
    validateSingleField,
  };
}
