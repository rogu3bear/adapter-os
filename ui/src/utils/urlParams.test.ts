import { describe, expect, it } from 'vitest';
import { parsePreselectParams, removeParams } from './urlParams';

describe('parsePreselectParams', () => {
  it('extracts adapterId and datasetId from query', () => {
    const { adapterId, datasetId } = parsePreselectParams('?adapterId=a1&datasetId=d1');
    expect(adapterId).toBe('a1');
    expect(datasetId).toBe('d1');
  });

  it('returns undefined when params are missing', () => {
    const { adapterId, datasetId } = parsePreselectParams('');
    expect(adapterId).toBeUndefined();
    expect(datasetId).toBeUndefined();
  });

  it('falls back to hash for adapterId if not in query', () => {
    const { adapterId, datasetId } = parsePreselectParams('?datasetId=d1', '#adapterId=a2');
    expect(adapterId).toBe('a2');
    expect(datasetId).toBe('d1');
  });
});

describe('removeParams', () => {
  it('removes specified params and keeps others', () => {
    const result = removeParams('?adapterId=a1&datasetId=d1&foo=bar', ['adapterId', 'datasetId']);
    expect(result).toBe('?foo=bar');
  });

  it('returns empty string when no params remain', () => {
    const result = removeParams('?adapterId=a1', ['adapterId']);
    expect(result).toBe('');
  });
});

