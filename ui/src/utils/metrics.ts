export interface FormatMetricOptions {
  decimals?: number;
  suffix?: string;
  placeholder?: string;
}

/**
 * Format metric values consistently across dashboard tiles.
 * Falls back to a placeholder for null/undefined/NaN/Infinity.
 */
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

/**
 * Check if at least one metric value is usable.
 */
export function hasUsableMetric(values: Array<number | null | undefined>): boolean {
  return values.some(v => v !== null && v !== undefined && Number.isFinite(v));
}

