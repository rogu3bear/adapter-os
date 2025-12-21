import { describe, expect, it } from 'vitest';
import { resolveTrainingTab, trainingTabToPath } from '@/pages/Training/tabs';

describe('Training tab mapping', () => {
  it('resolves tabs from routes and hashes', () => {
    expect(resolveTrainingTab('/training/jobs', '', { jobId: 'job-1' })).toBe('jobs');
    expect(resolveTrainingTab('/training/jobs/job-1', '', { jobId: 'job-1' })).toBe('jobs');
    expect(resolveTrainingTab('/training/datasets/ds-1', '', { datasetId: 'ds-1' })).toBe('datasets');
    expect(resolveTrainingTab('/training/templates', '')).toBe('templates');
    expect(resolveTrainingTab('/training', '#artifacts')).toBe('artifacts');
    expect(resolveTrainingTab('/training', '#settings')).toBe('settings');
    expect(resolveTrainingTab('/training', '')).toBe('overview');
  });

  it('builds paths from tab enums', () => {
    expect(trainingTabToPath('overview')).toBe('/training');
    expect(trainingTabToPath('jobs')).toBe('/training/jobs');
    expect(trainingTabToPath('datasets')).toBe('/training/datasets');
    expect(trainingTabToPath('templates')).toBe('/training/templates');
    expect(trainingTabToPath('artifacts')).toBe('/training#artifacts');
    expect(trainingTabToPath('settings')).toBe('/training#settings');
  });
});


