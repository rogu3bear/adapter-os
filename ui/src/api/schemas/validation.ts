/**
 * Safe validation helpers for API responses.
 * These functions log validation errors but don't throw, allowing graceful degradation.
 */

import { z } from 'zod';

/**
 * Safely parse API response data using a Zod schema.
 * Logs validation errors but returns null instead of throwing.
 *
 * @param schema - Zod schema to validate against
 * @param data - Unknown data from API
 * @param context - Optional context for error logging (e.g., endpoint name)
 * @returns Parsed data if valid, null if invalid
 */
export function safeParseApiResponse<T>(
  schema: z.ZodSchema<T>,
  data: unknown,
  context?: string
): T | null {
  const result = schema.safeParse(data);

  if (result.success) {
    return result.data;
  }

  // Log validation error for debugging
  const contextStr = context ? ` (${context})` : '';
  console.error(`[API Validation Error]${contextStr}:`, {
    errors: result.error.issues,
    data,
  });

  return null;
}

/**
 * Safely parse API response data, throwing on validation failure.
 * Use this when you need strict validation and want to handle errors explicitly.
 *
 * @param schema - Zod schema to validate against
 * @param data - Unknown data from API
 * @param context - Optional context for error messages
 * @returns Parsed data
 * @throws ZodError if validation fails
 */
export function parseApiResponse<T>(
  schema: z.ZodSchema<T>,
  data: unknown,
  context?: string
): T {
  try {
    return schema.parse(data);
  } catch (error) {
    const contextStr = context ? ` (${context})` : '';
    console.error(`[API Validation Error]${contextStr}:`, error);
    throw error;
  }
}

/**
 * Safely parse an array of API responses.
 * Invalid items are filtered out and logged.
 *
 * @param schema - Zod schema for individual items
 * @param data - Unknown array data from API
 * @param context - Optional context for error logging
 * @returns Array of valid items (invalid items are filtered out)
 */
export function safeParseApiArray<T>(
  schema: z.ZodSchema<T>,
  data: unknown,
  context?: string
): T[] {
  if (!Array.isArray(data)) {
    const contextStr = context ? ` (${context})` : '';
    console.error(`[API Validation Error]${contextStr}: Expected array, got:`, typeof data);
    return [];
  }

  const results: T[] = [];
  data.forEach((item, index) => {
    const parsed = safeParseApiResponse(schema, item, `${context}[${index}]`);
    if (parsed !== null) {
      results.push(parsed);
    }
  });

  return results;
}

/**
 * Options for response validation
 */
export interface ValidateResponseOptions {
  /** If true, empty objects ({}) are valid and skip schema validation */
  allowEmpty?: boolean;
  /** Context string for error logging */
  context?: string;
}

/**
 * Validate an API response with support for empty responses (204 No Content).
 *
 * This helper handles the common case where 204 responses return an empty object
 * that shouldn't be validated against the expected response schema.
 *
 * @param schema - Zod schema to validate against
 * @param data - Unknown data from API response
 * @param options - Validation options
 * @returns Parsed data if valid, null if validation fails
 *
 * @example
 * // For an endpoint that may return 204 No Content:
 * const result = validateResponse(UserSchema, response, { allowEmpty: true });
 * if (result === null) {
 *   // Validation failed (not just empty)
 * }
 */
export function validateResponse<T>(
  schema: z.ZodSchema<T>,
  data: unknown,
  options: ValidateResponseOptions = {}
): T | null {
  const { allowEmpty = false, context } = options;

  // Handle empty responses (typically from 204 No Content)
  if (allowEmpty && isEmptyObject(data)) {
    return data as T;
  }

  return safeParseApiResponse(schema, data, context);
}

/**
 * Check if a value is an empty object {}
 */
function isEmptyObject(value: unknown): value is Record<string, never> {
  return (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    Object.keys(value).length === 0
  );
}
