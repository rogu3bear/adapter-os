/**
 * Type-safe transformers for snake_case ↔ camelCase conversions
 *
 * Provides both compile-time type transformations and runtime utilities
 * for converting between naming conventions while preserving type safety.
 *
 * @module transformers
 * @example
 * ```typescript
 * import { toCamelCase, toSnakeCase, transformAndValidate } from './transformers';
 *
 * // Runtime transformation
 * const apiResponse = { user_id: 1, first_name: 'John' };
 * const camelCased = toCamelCase(apiResponse);
 * // Type: { userId: number; firstName: string }
 *
 * // With validation
 * const schema = z.object({ user_id: z.number() });
 * const validated = transformAndValidate(schema, apiResponse);
 * // Type: { userId: number }
 * ```
 */

import type { z } from 'zod';

/* ============================================================================
 * TYPE-LEVEL TRANSFORMATIONS (Compile-time)
 * ========================================================================== */

/**
 * Convert a snake_case string literal to camelCase at the type level
 *
 * @example
 * ```typescript
 * type Result = SnakeToCamel<'user_id'>; // 'userId'
 * type Result2 = SnakeToCamel<'first_name_last'>; // 'firstNameLast'
 * ```
 */
export type SnakeToCamel<S extends string> = S extends `${infer T}_${infer U}`
  ? `${T}${Capitalize<SnakeToCamel<U>>}`
  : S;

/**
 * Convert a camelCase string literal to snake_case at the type level
 *
 * @example
 * ```typescript
 * type Result = CamelToSnake<'userId'>; // 'user_id'
 * type Result2 = CamelToSnake<'firstNameLast'>; // 'first_name_last'
 * ```
 */
export type CamelToSnake<S extends string> = S extends `${infer T}${infer U}`
  ? U extends Uncapitalize<U>
    ? `${Lowercase<T>}${CamelToSnake<U>}`
    : `${Lowercase<T>}_${CamelToSnake<Uncapitalize<U>>}`
  : S;

/**
 * Transform all object keys to camelCase recursively
 * Preserves arrays, dates, and null/undefined values
 *
 * @example
 * ```typescript
 * type Input = { user_id: number; user_data: { first_name: string } };
 * type Result = CamelCaseKeys<Input>;
 * // { userId: number; userData: { firstName: string } }
 * ```
 */
export type CamelCaseKeys<T> = T extends Array<infer U>
  ? Array<CamelCaseKeys<U>>
  : T extends Date
  ? T
  : T extends object
  ? {
      [K in keyof T as K extends string ? SnakeToCamel<K> : K]: CamelCaseKeys<T[K]>;
    }
  : T;

/**
 * Transform all object keys to snake_case recursively
 * Preserves arrays, dates, and null/undefined values
 *
 * @example
 * ```typescript
 * type Input = { userId: number; userData: { firstName: string } };
 * type Result = SnakeCaseKeys<Input>;
 * // { user_id: number; user_data: { first_name: string } }
 * ```
 */
export type SnakeCaseKeys<T> = T extends Array<infer U>
  ? Array<SnakeCaseKeys<U>>
  : T extends Date
  ? T
  : T extends object
  ? {
      [K in keyof T as K extends string ? CamelToSnake<K> : K]: SnakeCaseKeys<T[K]>;
    }
  : T;

/* ============================================================================
 * RUNTIME TRANSFORMATIONS
 * ========================================================================== */

/**
 * Convert a snake_case string to camelCase at runtime
 *
 * @param str - The snake_case string to convert
 * @returns The camelCase string
 *
 * @example
 * ```typescript
 * snakeToCamelCase('user_id'); // 'userId'
 * snakeToCamelCase('first_name_last'); // 'firstNameLast'
 * snakeToCamelCase('field_0'); // 'field0'
 * snakeToCamelCase('user_id_123'); // 'userId_123'
 * ```
 */
function snakeToCamelCase(str: string): string {
  // Only capitalize letters after underscores, not numbers
  // But handle terminal numbers differently (field_0 -> field0)
  return str.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase())
           .replace(/_(\d+)$/g, '$1'); // Remove underscore before terminal numbers
}

/**
 * Check if a string is in camelCase format (starts with lowercase)
 */
function isCamelCaseString(str: string): boolean {
  // Must start with lowercase letter and contain at least one uppercase letter
  return /^[a-z][a-zA-Z0-9]*$/.test(str) && /[A-Z]/.test(str);
}

/**
 * Convert a camelCase string to snake_case at runtime
 *
 * @param str - The camelCase string to convert
 * @returns The snake_case string
 *
 * @example
 * ```typescript
 * camelToSnakeCase('userId'); // 'user_id'
 * camelToSnakeCase('firstNameLast'); // 'first_name_last'
 * camelToSnakeCase('CONSTANT'); // 'CONSTANT' (preserved)
 * ```
 */
function camelToSnakeCase(str: string): string {
  // Only transform if it's actually camelCase
  if (!isCamelCaseString(str)) {
    return str;
  }
  return str.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
}

/**
 * Check if a value is a plain object (not array, date, null, etc.)
 *
 * @param value - The value to check
 * @returns True if the value is a plain object
 */
function isPlainObject(value: unknown): value is Record<string, unknown> {
  return (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    !(value instanceof Date) &&
    !(value instanceof RegExp) &&
    !(value instanceof Map) &&
    !(value instanceof Set) &&
    // Handle browser-specific types
    (typeof File === 'undefined' || !(value instanceof File)) &&
    (typeof Blob === 'undefined' || !(value instanceof Blob)) &&
    (typeof FormData === 'undefined' || !(value instanceof FormData))
  );
}

/**
 * Transform an object's keys from snake_case to camelCase at runtime
 * Recursively transforms nested objects and arrays
 *
 * @param obj - The object to transform
 * @param visited - WeakSet to track visited objects (prevents circular references)
 * @returns A new object with camelCase keys
 *
 * @example
 * ```typescript
 * const input = {
 *   user_id: 1,
 *   user_data: {
 *     first_name: 'John',
 *     last_name: 'Doe'
 *   },
 *   tags: ['admin_user', 'power_user']
 * };
 *
 * const result = toCamelCase(input);
 * // {
 * //   userId: 1,
 * //   userData: {
 * //     firstName: 'John',
 * //     lastName: 'Doe'
 * //   },
 * //   tags: ['admin_user', 'power_user'] // strings in arrays not transformed
 * // }
 * ```
 */
export function toCamelCase<T>(obj: T, visited?: WeakSet<object>): CamelCaseKeys<T> {
  // Handle null/undefined
  if (obj === null || obj === undefined) {
    return obj as CamelCaseKeys<T>;
  }

  // Handle arrays
  if (Array.isArray(obj)) {
    return obj.map((item) => toCamelCase(item, visited)) as CamelCaseKeys<T>;
  }

  // Handle dates and other special objects
  if (obj instanceof Date || !isPlainObject(obj)) {
    return obj as CamelCaseKeys<T>;
  }

  // Initialize visited set on first call
  const visitedSet = visited || new WeakSet<object>();

  // Detect circular references
  if (visitedSet.has(obj as object)) {
    return obj as CamelCaseKeys<T>;
  }

  // Mark as visited
  visitedSet.add(obj as object);

  // Handle plain objects
  const result: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(obj)) {
    const camelKey = snakeToCamelCase(key);
    result[camelKey] = toCamelCase(value, visitedSet);
  }

  return result as CamelCaseKeys<T>;
}

/**
 * Transform an object's keys from camelCase to snake_case at runtime
 * Recursively transforms nested objects and arrays
 *
 * @param obj - The object to transform
 * @param visited - WeakSet to track visited objects (prevents circular references)
 * @returns A new object with snake_case keys
 *
 * @example
 * ```typescript
 * const input = {
 *   userId: 1,
 *   userData: {
 *     firstName: 'John',
 *     lastName: 'Doe'
 *   },
 *   createdAt: new Date()
 * };
 *
 * const result = toSnakeCase(input);
 * // {
 * //   user_id: 1,
 * //   user_data: {
 * //     first_name: 'John',
 * //     last_name: 'Doe'
 * //   },
 * //   created_at: Date // Date object preserved
 * // }
 * ```
 */
export function toSnakeCase<T>(obj: T, visited?: WeakSet<object>): SnakeCaseKeys<T> {
  // Handle null/undefined
  if (obj === null || obj === undefined) {
    return obj as SnakeCaseKeys<T>;
  }

  // Handle arrays
  if (Array.isArray(obj)) {
    return obj.map((item) => toSnakeCase(item, visited)) as SnakeCaseKeys<T>;
  }

  // Handle dates and other special objects
  if (obj instanceof Date || !isPlainObject(obj)) {
    return obj as SnakeCaseKeys<T>;
  }

  // Initialize visited set on first call
  const visitedSet = visited || new WeakSet<object>();

  // Detect circular references
  if (visitedSet.has(obj as object)) {
    return obj as SnakeCaseKeys<T>;
  }

  // Mark as visited
  visitedSet.add(obj as object);

  // Handle plain objects
  const result: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(obj)) {
    const snakeKey = camelToSnakeCase(key);
    result[snakeKey] = toSnakeCase(value, visitedSet);
  }

  return result as SnakeCaseKeys<T>;
}

/* ============================================================================
 * VALIDATION-AWARE TRANSFORMERS
 * ========================================================================== */

/**
 * Transform data from snake_case to camelCase and validate against a Zod schema
 *
 * This function first validates the snake_case data against the schema,
 * then transforms the validated result to camelCase. This ensures type safety
 * and runtime validation before transformation.
 *
 * @param schema - The Zod schema to validate against (expects snake_case keys)
 * @param data - The data to transform and validate
 * @returns The validated and transformed data with camelCase keys
 * @throws {z.ZodError} If validation fails
 *
 * @example
 * ```typescript
 * import { z } from 'zod';
 *
 * const UserSchema = z.object({
 *   user_id: z.number(),
 *   first_name: z.string(),
 *   created_at: z.string().datetime()
 * });
 *
 * const apiResponse = {
 *   user_id: 1,
 *   first_name: 'John',
 *   created_at: '2024-01-01T00:00:00Z'
 * };
 *
 * const validated = transformAndValidate(UserSchema, apiResponse);
 * // Type: { userId: number; firstName: string; createdAt: string }
 * // Runtime: validates then transforms to camelCase
 * ```
 */
export function transformAndValidate<T extends z.ZodType>(
  schema: T,
  data: unknown
): CamelCaseKeys<z.infer<T>> {
  // First validate the data against the schema
  const validated = schema.parse(data);

  // Then transform to camelCase
  return toCamelCase(validated);
}

/**
 * Prepare request data by transforming from camelCase to snake_case
 * Optionally validates the camelCase data before transformation
 *
 * This is useful for transforming frontend data structures to backend API format.
 *
 * @param data - The camelCase data to transform
 * @param schema - Optional Zod schema to validate against (expects camelCase keys)
 * @returns The transformed data with snake_case keys
 * @throws {z.ZodError} If schema is provided and validation fails
 *
 * @example
 * ```typescript
 * import { z } from 'zod';
 *
 * const UpdateUserSchema = z.object({
 *   userId: z.number(),
 *   firstName: z.string(),
 *   lastName: z.string()
 * });
 *
 * const formData = {
 *   userId: 1,
 *   firstName: 'John',
 *   lastName: 'Doe'
 * };
 *
 * const payload = prepareRequest(formData, UpdateUserSchema);
 * // Type: { user_id: number; first_name: string; last_name: string }
 * // Runtime: validates camelCase then transforms to snake_case
 * ```
 */
export function prepareRequest<T>(
  data: T,
  schema?: z.ZodType
): SnakeCaseKeys<T> {
  // Validate if schema is provided
  if (schema) {
    schema.parse(data);
  }

  // Transform to snake_case
  return toSnakeCase(data);
}

/* ============================================================================
 * BATCH TRANSFORMERS
 * ========================================================================== */

/**
 * Transform an array of objects from snake_case to camelCase
 *
 * @param items - The array of objects to transform
 * @returns A new array with all objects transformed to camelCase
 *
 * @example
 * ```typescript
 * const users = [
 *   { user_id: 1, first_name: 'John' },
 *   { user_id: 2, first_name: 'Jane' }
 * ];
 *
 * const transformed = toCamelCaseBatch(users);
 * // [
 * //   { userId: 1, firstName: 'John' },
 * //   { userId: 2, firstName: 'Jane' }
 * // ]
 * ```
 */
export function toCamelCaseBatch<T>(items: T[]): CamelCaseKeys<T>[] {
  return items.map((item) => toCamelCase(item));
}

/**
 * Transform an array of objects from camelCase to snake_case
 *
 * @param items - The array of objects to transform
 * @returns A new array with all objects transformed to snake_case
 *
 * @example
 * ```typescript
 * const users = [
 *   { userId: 1, firstName: 'John' },
 *   { userId: 2, firstName: 'Jane' }
 * ];
 *
 * const transformed = toSnakeCaseBatch(users);
 * // [
 * //   { user_id: 1, first_name: 'John' },
 * //   { user_id: 2, first_name: 'Jane' }
 * // ]
 * ```
 */
export function toSnakeCaseBatch<T>(items: T[]): SnakeCaseKeys<T>[] {
  return items.map((item) => toSnakeCase(item));
}

/* ============================================================================
 * UTILITY HELPERS
 * ========================================================================== */

/**
 * Create a transformer function that can be reused
 * Useful for creating consistent transformers for specific types
 *
 * @example
 * ```typescript
 * const userTransformer = createTransformer<ApiUser>();
 *
 * const apiResponse = { user_id: 1, first_name: 'John' };
 * const user = userTransformer.toCamelCase(apiResponse);
 *
 * const payload = userTransformer.toSnakeCase(user);
 * ```
 */
export function createTransformer<T>() {
  return {
    toCamelCase: (obj: T): CamelCaseKeys<T> => toCamelCase(obj),
    toSnakeCase: (obj: CamelCaseKeys<T>): SnakeCaseKeys<CamelCaseKeys<T>> =>
      toSnakeCase(obj),
    toCamelCaseBatch: (items: T[]): CamelCaseKeys<T>[] => toCamelCaseBatch(items),
    toSnakeCaseBatch: (items: CamelCaseKeys<T>[]): SnakeCaseKeys<CamelCaseKeys<T>>[] =>
      toSnakeCaseBatch(items),
  };
}

/* ============================================================================
 * TYPE GUARDS
 * ========================================================================== */

/**
 * Check if all keys in an object are in snake_case format
 *
 * @param obj - The object to check
 * @returns True if all keys are snake_case
 *
 * @example
 * ```typescript
 * isSnakeCase({ user_id: 1 }); // true
 * isSnakeCase({ userId: 1 }); // false
 * isSnakeCase({ user_id: 1, firstName: 'John' }); // false (mixed)
 * ```
 */
export function isSnakeCase(obj: unknown): boolean {
  if (!isPlainObject(obj)) {
    return false;
  }

  const snakeCasePattern = /^[a-z][a-z0-9_]*$/;

  for (const key of Object.keys(obj)) {
    if (!snakeCasePattern.test(key)) {
      return false;
    }

    // Recursively check nested objects
    const value = obj[key];
    if (isPlainObject(value) && !isSnakeCase(value)) {
      return false;
    }
    if (Array.isArray(value)) {
      for (const item of value) {
        if (isPlainObject(item) && !isSnakeCase(item)) {
          return false;
        }
      }
    }
  }

  return true;
}

/**
 * Check if all keys in an object are in camelCase format
 *
 * @param obj - The object to check
 * @returns True if all keys are camelCase
 *
 * @example
 * ```typescript
 * isCamelCase({ userId: 1 }); // true
 * isCamelCase({ user_id: 1 }); // false
 * isCamelCase({ userId: 1, first_name: 'John' }); // false (mixed)
 * ```
 */
export function isCamelCase(obj: unknown): boolean {
  if (!isPlainObject(obj)) {
    return false;
  }

  const camelCasePattern = /^[a-z][a-zA-Z0-9]*$/;

  for (const key of Object.keys(obj)) {
    if (!camelCasePattern.test(key)) {
      return false;
    }

    // Recursively check nested objects
    const value = obj[key];
    if (isPlainObject(value) && !isCamelCase(value)) {
      return false;
    }
    if (Array.isArray(value)) {
      for (const item of value) {
        if (isPlainObject(item) && !isCamelCase(item)) {
          return false;
        }
      }
    }
  }

  return true;
}
