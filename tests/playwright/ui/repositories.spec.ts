import { test, expect } from '@playwright/test';
import { gotoAndBootstrap, seeded } from './utils';

test('repositories list and detail', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/repositories', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Repositories', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText(seeded.repoId, { exact: true })).toBeVisible();

  await gotoAndBootstrap(page, `/repositories/${seeded.repoId}`, { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Repository Details', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Information')).toBeVisible();
});
