/**
 * useFormValidation hook
 * Provides real-time validation for forms using Zod schemas
 * Works seamlessly with react-hook-form
 */

import { useState, useCallback } from 'react';
import { ZodSchema, ZodError } from 'zod';
import { formatValidationError, ValidationResult, validateField } from '../schemas/utils';

export interface UseFormValidationOptions {
  // Enable real-time validation on every change
  realtime?: boolean;
  // Debounce delay for real-time validation (ms)
  debounceDelay?: number;
}

export interface UseFormValidationResult {
  // Current validation result
  validationResult: ValidationResult | null;

  // Quick check: are there any errors?
  hasErrors: boolean;

  // Validate entire form
  validate: (data: any) => Promise<ValidationResult>;

  // Validate single field
  validateField: (fieldName: string, value: any) => void;

  // Get error for specific field
  getFieldError: (fieldName: string) => string | undefined;

  // Clear all errors
  clearErrors: () => void;

  // Clear specific field error
  clearFieldError: (fieldName: string) => void;

  // Get all field errors as object
  getFieldErrors: () => Record<string, string>;

  // Set custom error for a field
  setFieldError: (fieldName: string, error: string) => void;
}

/**
 * Custom hook for form validation using Zod schemas
 * Integrates with react-hook-form or standalone
 */
export function useFormValidation(
  schema: ZodSchema,
  options: UseFormValidationOptions = {}
): UseFormValidationResult {
  const { realtime = false, debounceDelay = 300 } = options;
  const [validationResult, setValidationResult] = useState<ValidationResult | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const debounceTimerRef = React.useRef<NodeJS.Timeout | null>(null);

  /**
   * Validate entire form data
   */
  const validate = useCallback(async (data: any): Promise<ValidationResult> => {
    try {
      await schema.parseAsync(data);
      const result: ValidationResult = {
        success: true,
        errors: [],
        fieldErrors: {},
      };
      setValidationResult(result);
      setFieldErrors({});
      return result;
    } catch (error) {
      if (error instanceof ZodError) {
        const result = formatValidationError(error);
        setValidationResult(result);
        setFieldErrors(result.fieldErrors);
        return result;
      }
      const result: ValidationResult = {
        success: false,
        errors: [{ field: 'unknown', message: 'Validation failed' }],
        fieldErrors: { unknown: 'Validation failed' },
      };
      setValidationResult(result);
      setFieldErrors(result.fieldErrors);
      return result;
    }
  }, [schema]);

  /**
   * Validate single field with real-time support
   */
  const validateFieldValue = useCallback((fieldName: string, value: any) => {
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
    }

    if (realtime) {
      // Use debounce for real-time validation
      debounceTimerRef.current = setTimeout(() => {
        const result = validateField(schema, fieldName, value);
        if (result.valid) {
          // Remove field error if validation passed
          setFieldErrors((prev) => {
            const updated = { ...prev };
            delete updated[fieldName];
            return updated;
          });
        } else {
          // Add field error
          setFieldErrors((prev) => ({
            ...prev,
            [fieldName]: result.error || 'Validation failed',
          }));
        }
      }, debounceDelay);
    } else {
      // Immediate validation if not real-time
      const result = validateField(schema, fieldName, value);
      if (result.valid) {
        setFieldErrors((prev) => {
          const updated = { ...prev };
          delete updated[fieldName];
          return updated;
        });
      } else {
        setFieldErrors((prev) => ({
          ...prev,
          [fieldName]: result.error || 'Validation failed',
        }));
      }
    }
  }, [schema, realtime, debounceDelay]);

  /**
   * Get error for specific field
   */
  const getFieldError = useCallback((fieldName: string): string | undefined => {
    return fieldErrors[fieldName];
  }, [fieldErrors]);

  /**
   * Clear all errors
   */
  const clearErrors = useCallback(() => {
    setValidationResult(null);
    setFieldErrors({});
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
    }
  }, []);

  /**
   * Clear specific field error
   */
  const clearFieldError = useCallback((fieldName: string) => {
    setFieldErrors((prev) => {
      const updated = { ...prev };
      delete updated[fieldName];
      return updated;
    });
  }, []);

  /**
   * Get all field errors as object
   */
  const getFieldErrors = useCallback((): Record<string, string> => {
    return { ...fieldErrors };
  }, [fieldErrors]);

  /**
   * Set custom error for a field
   */
  const setFieldError = useCallback((fieldName: string, error: string) => {
    setFieldErrors((prev) => ({
      ...prev,
      [fieldName]: error,
    }));
  }, []);

  return {
    validationResult,
    hasErrors: Object.keys(fieldErrors).length > 0,
    validate,
    validateField: validateFieldValue,
    getFieldError,
    clearErrors,
    clearFieldError,
    getFieldErrors,
    setFieldError,
  };
}

// Import React for useRef
import React from 'react';
