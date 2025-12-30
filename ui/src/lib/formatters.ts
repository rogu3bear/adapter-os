/**
 * Unified formatting utilities
 *
 * Consolidates duplicated formatDuration, formatBytes, formatTimestamp functions.
 * Replaces scattered implementations in history-utils.ts, trainingEta.ts, and inline functions.
 */

/**
 * Format duration from milliseconds to human-readable string
 * Use for elapsed times, API response times, etc.
 */
export function formatDurationMs(ms: number): string {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  if (ms < 60000) {
    return `${(ms / 1000).toFixed(2)}s`;
  }
  const minutes = ms / 60000;
  return `${minutes.toFixed(2)}m`;
}

/**
 * Format duration from seconds to human-readable string
 * Use for training times, ETA, longer durations.
 */
export function formatDurationSeconds(seconds: number | null | undefined): string {
  if (!seconds || seconds <= 0) return '-';

  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);

  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${secs}s`;
  }
  return `${secs}s`;
}

/**
 * Format duration with explicit unit specification
 * @param value - The duration value
 * @param unit - 'ms' for milliseconds, 's' for seconds
 */
export function formatDuration(value: number | null | undefined, unit: 'ms' | 's' = 's'): string {
  if (value === null || value === undefined) return '-';
  return unit === 'ms' ? formatDurationMs(value) : formatDurationSeconds(value);
}

/**
 * Format bytes to human-readable string
 * Handles KB, MB, GB with appropriate precision
 * Returns '—' for null/undefined values
 */
export function formatBytes(bytes: number | null | undefined, decimals = 2): string {
  if (bytes == null) return '—';
  if (bytes === 0) return '0 B';
  if (bytes < 0) return '—';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const value = bytes / Math.pow(k, i);

  // Use more precision for smaller values
  const precision = value < 10 ? 2 : value < 100 ? 1 : 0;
  return `${value.toFixed(precision)} ${units[i]}`;
}

/**
 * Format bytes specifically to MB.
 * Returns '—' for null/undefined values.
 */
export function formatMB(bytes: number | undefined | null, decimals = 2): string {
  if (bytes == null) return '—';
  return `${(bytes / 1024 / 1024).toFixed(decimals)} MB`;
}

/**
 * Format bytes to GB (from MB input).
 * Returns '—' for null/undefined values.
 */
export function formatGB(mb: number | undefined | null, decimals = 1): string {
  if (mb == null) return '—';
  return `${(mb / 1024).toFixed(decimals)} GB`;
}

/**
 * Format timestamp to human-readable string with explicit timezone
 * @param timestamp - Unix timestamp in milliseconds or ISO string
 * @param format - 'short' for time only, 'long' for full date+time, 'iso' for ISO 8601 UTC
 */
export function formatTimestamp(
  timestamp: number | string,
  format: 'short' | 'long' | 'iso' = 'short'
): string {
  const date = typeof timestamp === 'string' ? new Date(timestamp) : new Date(timestamp);

  if (isNaN(date.getTime())) {
    return '-';
  }

  if (format === 'iso') {
    return date.toISOString();
  }

  // Use explicit locale and timezone options for consistency
  const options: Intl.DateTimeFormatOptions = {
    timeZone: 'UTC',
    timeZoneName: 'short',
  };

  if (format === 'short') {
    return date.toLocaleTimeString('en-US', { ...options, hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  return date.toLocaleString('en-US', { ...options, dateStyle: 'medium', timeStyle: 'medium' });
}

/**
 * Format relative time with explicit calendar date anchor
 * Returns both relative time AND the explicit date to avoid ambiguity
 * @param date - The date to format
 * @param includeAnchor - If true, always append the calendar date (default: true for >1 hour)
 */
export function formatRelativeTime(date: Date | string | number, includeAnchor = true): string {
  const now = new Date();
  const target = date instanceof Date ? date : new Date(date);
  const diffMs = now.getTime() - target.getTime();
  const diffSeconds = Math.floor(diffMs / 1000);
  const diffMinutes = Math.floor(diffSeconds / 60);
  const diffHours = Math.floor(diffMinutes / 60);
  const diffDays = Math.floor(diffHours / 24);

  // Format calendar date in UTC for anchoring
  const calendarDate = target.toLocaleDateString('en-US', {
    timeZone: 'UTC',
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });

  if (diffSeconds < 60) {
    return includeAnchor ? `just now (${calendarDate})` : 'just now';
  }
  if (diffMinutes < 60) {
    const relative = `${diffMinutes} minute${diffMinutes === 1 ? '' : 's'} ago`;
    return includeAnchor ? `${relative} (${calendarDate})` : relative;
  }
  if (diffHours < 24) {
    const relative = `${diffHours} hour${diffHours === 1 ? '' : 's'} ago`;
    // Always include anchor for durations > 1 hour to avoid confusion
    return `${relative} (${calendarDate})`;
  }
  if (diffDays < 7) {
    const relative = `${diffDays} day${diffDays === 1 ? '' : 's'} ago`;
    // Always include anchor for multi-day durations
    return `${relative} (${calendarDate})`;
  }

  // For older dates, just show the calendar date
  return calendarDate;
}

/**
 * Format percentage with consistent precision
 * Returns '—' for null/undefined values
 */
export function formatPercent(value: number | null | undefined, decimals: number = 1): string {
  if (value == null) return '—';
  return `${value.toFixed(decimals)}%`;
}

/**
 * Format number with thousands separators
 * Returns '—' for null/undefined values
 */
export function formatNumber(value: number | null | undefined): string {
  if (value == null) return '—';
  return value.toLocaleString();
}

/**
 * Format a count/number for display (alias for formatNumber).
 * Returns '—' for null/undefined values.
 */
export function formatCount(count: number | undefined | null): string {
  return formatNumber(count);
}

/**
 * Format a date string for display with explicit UTC timezone.
 * Returns '—' for null/undefined values.
 */
export function formatDate(date: string | Date | undefined | null): string {
  if (date == null) return '—';
  try {
    const d = typeof date === 'string' ? new Date(date) : date;
    return d.toLocaleDateString('en-US', {
      timeZone: 'UTC',
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return '—';
  }
}

/**
 * Format a datetime string for display with explicit UTC timezone.
 * Returns '—' for null/undefined values.
 */
export function formatDateTime(date: string | Date | undefined | null): string {
  if (date == null) return '—';
  try {
    const d = typeof date === 'string' ? new Date(date) : date;
    return d.toLocaleString('en-US', {
      timeZone: 'UTC',
      timeZoneName: 'short',
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  } catch {
    return '—';
  }
}

/**
 * Format a string with fallback.
 * Returns '—' for null/undefined/empty values.
 */
export function formatString(value: string | undefined | null): string {
  if (value == null || value === '') return '—';
  return value;
}

/**
 * Format metric value with optional decimals and suffix
 */
export interface FormatMetricOptions {
  decimals?: number;
  suffix?: string;
  placeholder?: string;
}

export function formatMetricValue(
  value: number | null | undefined,
  options: FormatMetricOptions = {},
): string {
  const { decimals, suffix = '', placeholder = 'N/A' } = options;

  if (value === null || value === undefined || Number.isNaN(value) || !Number.isFinite(value)) {
    return placeholder;
  }

  const formatted = typeof decimals === 'number' ? value.toFixed(decimals) : String(value);
  return suffix ? `${formatted}${suffix}` : formatted;
}
