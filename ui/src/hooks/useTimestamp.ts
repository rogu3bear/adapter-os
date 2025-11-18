/**
 * Canonical timestamp rendering hooks
 * 
 * Ensures all timestamps are displayed in ISO-8601 UTC format with fixed precision
 * for deterministic UI rendering.
 */


/**
 * Formats a timestamp string to ISO-8601 UTC format.
 *
 * @param timestamp - ISO timestamp string or undefined
 * @returns ISO-8601 formatted string (e.g., "2025-01-13T14:32:01.234Z") or "—" if invalid/missing
 */

>
export function useTimestamp(timestamp?: string): string {
  if (!timestamp) return '—';
  
  // Always render ISO-8601 in UTC with fixed millisecond precision
  const date = new Date(timestamp);
  if (isNaN(date.getTime())) return 'Invalid';
  
  return date.toISOString(); // e.g., "2025-01-13T14:32:01.234Z"
}


/**
 * Formats a timestamp string to relative time format.
 *
 * @param timestamp - ISO timestamp string or undefined
 * @returns Relative time string (e.g., "5s ago", "2m ago", "3h ago", "2d ago") or "—" if invalid/missing
 */

>
export function useRelativeTime(timestamp?: string): string {
  if (!timestamp) return '—';
  
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  
  if (diff < 60000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
  return `${Math.floor(diff / 86400000)}d ago`;
}

export function formatTimestamp(timestamp?: string): string {
  return useTimestamp(timestamp);
}

