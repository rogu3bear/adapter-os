import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Stable sort implementation for deterministic ordering
 */
export function stableSort<T>(
  items: T[],
  keys: (keyof T)[]
): T[] {
  return [...items].sort((a, b) => {
    for (const key of keys) {
      const aVal = a[key];
      const bVal = b[key];
      if (aVal < bVal) return -1;
      if (aVal > bVal) return 1;
    }
    return 0;
  });
}

/**
 * Generate canonical key for React list items
 */
export function canonicalKey(obj: any): string {
  return obj.hash || obj.id || obj.hash_b3 || JSON.stringify(obj);
}
