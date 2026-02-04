import { test, expect } from '@playwright/test';
import { ensureLoggedIn, seeded, waitForAppReady } from './utils';

test('repositories list and detail', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await page.goto('/repositories', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Repositories', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText(seeded.repoId, { exact: true })).toBeVisible();

  await page.goto(`/repositories/${seeded.repoId}`, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Repository Details', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Information')).toBeVisible();
});
