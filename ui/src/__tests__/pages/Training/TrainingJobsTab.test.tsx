import { describe, expect, it } from 'vitest';
import { filterJobsByAdapter } from '@/pages/Training/TrainingJobsTab';
import type { TrainingJob } from '@/api/training-types';

const jobs: TrainingJob[] = [
  { id: 'j1', adapter_id: 'adapter-1', adapter_name: 'Adapter One' } as TrainingJob,
  { id: 'j2', adapter_id: 'adapter-2', adapter_name: 'Adapter Two' } as TrainingJob,
];

describe('TrainingJobsTab helpers', () => {
  it('filters jobs by adapterId (id match)', () => {
    const filtered = filterJobsByAdapter(jobs, 'adapter-1');
    expect(filtered).toHaveLength(1);
    expect(filtered[0].id).toBe('j1');
  });

  it('filters jobs by adapterId (name match)', () => {
    const filtered = filterJobsByAdapter(jobs, 'adapter two');
    expect(filtered).toHaveLength(1);
    expect(filtered[0].id).toBe('j2');
  });

  it('returns all jobs when no adapterId provided', () => {
    const filtered = filterJobsByAdapter(jobs, undefined);
    expect(filtered).toHaveLength(2);
  });
});

