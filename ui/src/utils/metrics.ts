// Re-export formatMetricValue from formatters for backward compatibility
export { formatMetricValue, type FormatMetricOptions } from '@/lib/formatters';

/**
 * Check if at least one metric value is usable.
 */
export function hasUsableMetric(values: Array<number | null | undefined>): boolean {
  return values.some(v => v !== null && v !== undefined && Number.isFinite(v));
}

