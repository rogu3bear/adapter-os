/**
 * Sanitization utilities for sensitive data.
 *
 * This module provides utilities to sanitize form values and other objects
 * that may contain sensitive information before logging or displaying them.
 *
 * @module sanitize
 */

/**
 * Default list of sensitive field name patterns.
 * Field names containing these substrings (case-insensitive) will be redacted.
 */
const SENSITIVE_FIELDS = [
  'password',
  'secret',
  'token',
  'apikey',
  'api_key',
  'credential',
  'key',
  'auth',
  'totp',
  'otp',
  'mfa',
  'private',
];

/**
 * Sanitizes an object by redacting sensitive field values.
 *
 * This function creates a shallow copy of the input object and replaces
 * the values of any fields that match sensitive patterns with '[REDACTED]'.
 *
 * @param values - The object to sanitize
 * @param additionalFields - Additional field name patterns to treat as sensitive
 * @returns A new object with sensitive values redacted
 *
 * @example
 * ```typescript
 * const formData = { email: 'user@example.com', password: 'secret123' };
 * const sanitized = sanitizeFormValues(formData);
 * // { email: 'user@example.com', password: '[REDACTED]' }
 * ```
 *
 * @example
 * ```typescript
 * const data = { username: 'john', sessionId: 'xyz' };
 * const sanitized = sanitizeFormValues(data, ['session']);
 * // { username: 'john', sessionId: '[REDACTED]' }
 * ```
 */
export function sanitizeFormValues<T extends Record<string, unknown>>(
  values: T,
  additionalFields: string[] = []
): Record<string, unknown> {
  const sensitiveKeys = [...SENSITIVE_FIELDS, ...additionalFields];
  const sanitized: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(values)) {
    const keyLower = key.toLowerCase();
    const isSensitive = sensitiveKeys.some((field) =>
      keyLower.includes(field.toLowerCase())
    );

    if (isSensitive) {
      sanitized[key] = '[REDACTED]';
    } else if (value && typeof value === 'object' && !Array.isArray(value)) {
      // Recursively sanitize nested objects
      sanitized[key] = sanitizeFormValues(
        value as Record<string, unknown>,
        additionalFields
      );
    } else {
      sanitized[key] = value;
    }
  }

  return sanitized;
}

/**
 * Extracts only the field names from an object, useful for logging validation errors
 * without exposing any actual values.
 *
 * @param values - The object to extract field names from
 * @returns An array of field names
 *
 * @example
 * ```typescript
 * const formData = { email: 'user@example.com', password: 'secret123' };
 * const fields = getFieldNames(formData);
 * // ['email', 'password']
 * ```
 */
export function getFieldNames<T extends Record<string, unknown>>(
  values: T
): string[] {
  return Object.keys(values);
}
