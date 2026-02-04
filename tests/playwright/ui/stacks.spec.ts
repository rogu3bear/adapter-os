import { test, expect } from '@playwright/test';
import { seeded, waitForAppReady } from './utils';

test('stacks list and detail', async ({ page }) => {
  await page.goto('/stacks', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Runtime Stacks', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('stack.test')).toBeVisible();

  await page.goto(`/stacks/${seeded.stackId}`, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Stack Details', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Basic Information')).toBeVisible();
});
