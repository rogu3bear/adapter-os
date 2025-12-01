/**
 * Type Utilities
 *
 * Safe replacements for common `as any` patterns. These utilities provide
 * type-safe alternatives for working with unknown types, dynamic objects,
 * and runtime type checks without resorting to unsafe type assertions.
 *
 * Use these utilities to:
 * - Type-guard unknown API responses
 * - Safely access object properties
 * - Perform exhaustive checks in switch/if statements
 * - Work with dynamic keys and entries in a type-safe manner
 *
 * @module types/utilities
 */

/**
 * Type for API responses or objects with unknown structure.
 * Use this instead of `any` when the shape is not known at compile time.
 *
 * @example
 * const response: UnknownRecord = await fetch('/api/data').then(r => r.json());
 * if (hasProperty(response, 'status')) {
 *   console.log(response.status);
 * }
 */
export type UnknownRecord = Record<string, unknown>;

/**
 * Type-safe Object.keys that preserves the key types.
 * Returns an array of keys typed as `keyof T` instead of `string[]`.
 *
 * @example
 * const obj = { name: 'Alice', age: 30 };
 * const keys = typedKeys(obj); // ('name' | 'age')[]
 */
export function typedKeys<T extends object>(obj: T): (keyof T)[] {
  return Object.keys(obj) as (keyof T)[];
}

/**
 * Type-safe Object.entries that preserves key and value types.
 * Returns entries with proper typing instead of `[string, any][]`.
 *
 * @example
 * const obj = { name: 'Alice', age: 30 };
 * const entries = typedEntries(obj); // ['name', string] | ['age', number]
 * entries.forEach(([key, value]) => {
 *   // key and value are properly typed
 * });
 */
export function typedEntries<T extends object>(
  obj: T
): [keyof T, T[keyof T]][] {
  return Object.entries(obj) as [keyof T, T[keyof T]][];
}

/**
 * Type guard for checking if an object has a specific property.
 * Narrows the type to include the property if it exists.
 *
 * @example
 * function processResponse(data: unknown) {
 *   if (hasProperty(data, 'status') && typeof data.status === 'string') {
 *     console.log(data.status); // TypeScript knows data has status property
 *   }
 * }
 */
export function hasProperty<K extends string>(
  obj: unknown,
  key: K
): obj is { [P in K]: unknown } {
  return typeof obj === 'object' && obj !== null && key in obj;
}

/**
 * Type guard for checking if a value is a plain object.
 * Excludes arrays, null, and other non-object types.
 *
 * @example
 * function processValue(value: unknown) {
 *   if (isObject(value)) {
 *     // value is Record<string, unknown>
 *     typedKeys(value).forEach(key => {
 *       console.log(key, value[key]);
 *     });
 *   }
 * }
 */
export function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Type guard for filtering out null and undefined values.
 * Useful for array filtering and optional chaining.
 *
 * @example
 * const values = [1, null, 2, undefined, 3];
 * const numbers = values.filter(isNonNullable); // number[]
 *
 * @example
 * const maybeValue: string | null = getValue();
 * if (isNonNullable(maybeValue)) {
 *   console.log(maybeValue.toUpperCase()); // Safe to use
 * }
 */
export function isNonNullable<T>(value: T): value is NonNullable<T> {
  return value !== null && value !== undefined;
}

/**
 * Exhaustive check helper for switch statements and conditional logic.
 * Throws an error if called, indicating a case was not handled.
 * TypeScript will error at compile time if all cases aren't covered.
 *
 * @example
 * type Status = 'pending' | 'success' | 'error';
 *
 * function handleStatus(status: Status) {
 *   switch (status) {
 *     case 'pending':
 *       return 'Loading...';
 *     case 'success':
 *       return 'Done!';
 *     case 'error':
 *       return 'Failed!';
 *     default:
 *       return assertNever(status); // TypeScript errors if a case is missing
 *   }
 * }
 */
export function assertNever(value: never, message?: string): never {
  throw new Error(message ?? `Unexpected value: ${value}`);
}
