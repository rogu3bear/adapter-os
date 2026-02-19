/**
 * Skeleton demo operation — template for new operations.
 *
 * Copy this file to create a new demo operation:
 * 1. Copy to `operations/<name>.spec.ts`
 * 2. Update `meta` with your operation's id, title, and required mocks
 * 3. Implement the test body and `run()` function
 * 4. Re-export `meta` from `operations/index.ts`
 */

import { test } from '@playwright/test';
import type { DemoOperationMeta, DemoContext } from '../types';
import { createDemoContext, pacingFromEnv } from '../harness';
import { installMocks } from '../mocks';
import { gotoAndBootstrap, waitForAppReady } from '../../ui/utils';

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

test(meta.id, { tag: ['@demo'] }, async ({ page }) => {
  await installMocks(page, meta.mocks);
  const demo = createDemoContext(page, pacingFromEnv());

  await gotoAndBootstrap(page, '/dashboard', { mode: 'ui-only' });
  await waitForAppReady(page);

  await run(demo);
});
