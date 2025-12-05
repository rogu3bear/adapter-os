import { describe, expect, it } from 'vitest';
import { formatMetricValue, hasUsableMetric } from '@/utils/metrics';

describe('formatMetricValue', () => {
  it('formats finite numbers with optional decimals and suffix', () => {
    expect(formatMetricValue(1.2345)).toBe('1.2345');
    expect(formatMetricValue(1.2345, { decimals: 2 })).toBe('1.23');
    expect(formatMetricValue(42, { suffix: '%' })).toBe('42%');
    expect(formatMetricValue(3.1, { decimals: 1, suffix: ' req/s' })).toBe('3.1 req/s');
  });

  it('returns placeholder for nullish or non-finite values', () => {
    expect(formatMetricValue(null)).toBe('N/A');
    expect(formatMetricValue(undefined)).toBe('N/A');
    expect(formatMetricValue(Number.NaN)).toBe('N/A');
    expect(formatMetricValue(Number.POSITIVE_INFINITY)).toBe('N/A');
    expect(formatMetricValue(undefined, { placeholder: '—' })).toBe('—');
  });
});

describe('hasUsableMetric', () => {
  it('detects presence of finite values, including zero', () => {
    expect(hasUsableMetric([null, undefined])).toBe(false);
    expect(hasUsableMetric([0, null])).toBe(true);
    expect(hasUsableMetric([Number.POSITIVE_INFINITY, null])).toBe(false);
    expect(hasUsableMetric([1.2, null, undefined])).toBe(true);
  });
});

