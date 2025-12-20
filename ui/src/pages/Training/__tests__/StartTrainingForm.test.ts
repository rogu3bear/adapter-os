import { describe, expect, it } from 'vitest';

import { buildDatasetVersionSelections } from '@/pages/Training/StartTrainingForm';

describe('buildDatasetVersionSelections', () => {
  it('returns latest version with default weight', () => {
    const dataset = {
      id: 'ds-1',
      dataset_version_id: 'ver-123',
    } as const;

    const selections = buildDatasetVersionSelections(dataset);
    expect(selections).toEqual([{ dataset_version_id: 'ver-123', weight: 1 }]);
  });

  it('returns undefined when no version is available', () => {
    const dataset = { id: 'ds-2' } as const;
    const selections = buildDatasetVersionSelections(dataset);
    expect(selections).toBeUndefined();
  });
});
