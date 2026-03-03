import { test, expect } from '@playwright/test';
import { expectErrorState, gotoAndBootstrap } from './utils';

test('audit tabs render', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/audit', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Audit Log', level: 1, exact: true })
  ).toBeVisible();
  const timelineEmpty = page.getByText('No audit events found');
  if (await timelineEmpty.isVisible().catch(() => false)) {
    await expect(timelineEmpty).toBeVisible();
    return;
  }
  const totalEvents = page.getByText('Total Events');
  if (await totalEvents.isVisible().catch(() => false)) {
    await expect(totalEvents).toBeVisible();
    await expect(page.getByText('Chain Status')).toBeVisible();
    return;
  }
  await expectErrorState(page);
});
