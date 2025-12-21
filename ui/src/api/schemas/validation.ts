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
