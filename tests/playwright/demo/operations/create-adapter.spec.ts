import { test, expect } from '@playwright/test';
import type { DemoContext, DemoOperationMeta } from '../types';
import { createDemoContext, pacingFromEnv } from '../harness';
import { installMocks } from '../mocks';
import { gotoAndBootstrap, seeded, waitForAppReady } from '../../ui/utils';

export const meta: DemoOperationMeta = {
  id: 'create-adapter',
  title: 'Create Adapter From Dataset',
  mocks: ['system-ready', 'datasets-list'],
  tags: ['demo', 'training'],
};

export async function run(demo: DemoContext): Promise<void> {
  const { page } = demo;
  const createdSkillName = `demo-skill-${Date.now().toString().slice(-6)}`;
  const demoTrainingJobId = `demo-training-${Date.now().toString().slice(-6)}`;

  await page.route('**/v1/adapters/from-dataset/*', async (route) => {
    await route.fulfill({
      status: 202,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        id: demoTrainingJobId,
        adapter_name: createdSkillName,
      }),
    });
  });

  await demo.narrate('Open the training wizard and create a new adapter from dataset.');
  await page.getByRole('button', { name: 'Create Adapter', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Create Adapter', exact: true })).toBeVisible();

  const sourceSelects = page.locator('.wizard-step-content select');
  await sourceSelects.first().selectOption(seeded.datasetId);
  await page.getByRole('button', { name: 'Use selected dataset', exact: true }).click();
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await page.getByLabel('Adapter name').fill(createdSkillName);
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await page.getByRole('button', { name: 'Next', exact: true }).click();

  const canonicalStartResponse = page.waitForResponse(
    (resp) =>
      resp.request().method() === 'POST' &&
      /\/v1\/adapters\/from-dataset\/[^/]+$/.test(new URL(resp.url()).pathname),
    { timeout: 30_000 }
  );
  await page.getByRole('button', { name: 'Start training', exact: true }).click();
  const startResponse = await canonicalStartResponse;
  expect(startResponse.ok()).toBeTruthy();
  await demo.dwell(demo.pacing.afterAction);
}

test(meta.id, { tag: ['@demo'] }, async ({ page }) => {
  await installMocks(page, meta.mocks);
  const demo = createDemoContext(page, pacingFromEnv());

  await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
  await waitForAppReady(page);

  await run(demo);
});
