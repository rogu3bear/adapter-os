/**
 * Skeleton demo operation — template for new operations.
 *
 * Copy this file to create `operations/<name>.spec.ts`, then add your own
 * Playwright `test(...)` block in the new file.
 */

import type { DemoOperationMeta, DemoContext } from '../types';

export const meta: DemoOperationMeta = {
  id: 'skeleton',
  title: 'Skeleton Demo Operation',
  mocks: ['system-ready'],
  tags: ['skeleton'],
};

/**
 * Standalone run function for composition.
 * Call this from a multi-operation spec to run the skeleton as one step.
 */
export async function run(demo: DemoContext): Promise<void> {
  await demo.narrate('Welcome to AdapterOS');
  await demo.dwell(demo.pacing.finalDwell);
}
