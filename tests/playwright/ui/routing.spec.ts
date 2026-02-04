import { test, expect } from '@playwright/test';
import { waitForAppReady } from './utils';

test('routing management tab renders', async ({ page }) => {
  await page.goto('/routing', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Routing Debug', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Management')).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'No identity set selected' })
  ).toBeVisible();
});

test('routing decisions tab renders prompt input', async ({ page }) => {
  await page.goto('/routing', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await page.getByRole('tab', { name: 'Decisions' }).click();
  await expect(
    page.getByRole('heading', { name: 'Routing Decisions', level: 1, exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Debug Router' }).click();
  await expect(
    page.getByPlaceholder('Enter a prompt to test routing...')
  ).toBeVisible();
});
