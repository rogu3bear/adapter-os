import { test, expect } from '@playwright/test';
import { ensureLoggedIn, expectErrorState, waitForAppReady } from './utils';

test('audit tabs render', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/audit', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Audit Log', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Event Timeline' })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Hash Chain' })).toBeVisible();
  await page.getByRole('tab', { name: 'Hash Chain' }).click();
  const empty = page.getByText('No chain entries found');
  if (await empty.isVisible().catch(() => false)) {
    await expect(empty).toBeVisible();
    return;
  }
  const verification = page.getByText('Verification Status');
  if (await verification.isVisible().catch(() => false)) {
    await expect(verification).toBeVisible();
    return;
  }
  await expectErrorState(page);
});
