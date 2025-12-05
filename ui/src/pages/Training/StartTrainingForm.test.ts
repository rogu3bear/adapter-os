import { describe, expect, it } from 'vitest';
import { resolveDatasetPrefill } from './StartTrainingForm';
import type { Dataset } from '@/api/training-types';

const datasets: Dataset[] = [
  { id: 'ds1', name: 'One' } as Dataset,
  { id: 'ds2', name: 'Two' } as Dataset,
];

describe('StartTrainingForm helpers', () => {
  it('returns matching dataset id when present', () => {
    expect(resolveDatasetPrefill(datasets, 'ds2')).toBe('ds2');
  });

  it('returns undefined when dataset not found', () => {
    expect(resolveDatasetPrefill(datasets, 'missing')).toBeUndefined();
  });

  it('returns undefined when no id provided', () => {
    expect(resolveDatasetPrefill(datasets, undefined)).toBeUndefined();
  });
});

